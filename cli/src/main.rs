use std::{
	collections::HashSet,
	env::{self, var},
	path::PathBuf,
};

use dunce::canonicalize;
use miette::{IntoDiagnostic, Result};
use tracing::{debug, warn};
use watchexec::{
	event::Event,
	filter::tagged::{Filter, Matcher, Op, Pattern, Regex},
	ignore_files::{self, IgnoreFile},
	project::{self, ProjectType},
	Watchexec,
};

mod args;
mod config;

#[tokio::main]
async fn main() -> Result<()> {
	#[cfg(feature = "dev-console")]
	console_subscriber::init();

	if var("RUST_LOG").is_ok() && cfg!(not(feature = "dev-console")) {
		tracing_subscriber::fmt::init();
	}

	let args = args::get_args()?;

	tracing_subscriber::fmt()
		.with_env_filter(match args.occurrences_of("verbose") {
			0 => "watchexec-cli=warn",
			1 => "watchexec=debug,watchexec-cli=debug",
			2 => "watchexec=trace,watchexec-cli=trace",
			_ => "trace",
		})
		.try_init()
		.ok();

	let mut origins = HashSet::new();
	for path in args.values_of("paths").unwrap_or_default().into_iter() {
		let path = canonicalize(path).into_diagnostic()?;
		origins.extend(project::origins(&path).await);
	}

	debug!(?origins, "resolved all project origins");

	let project_origin = project::common_prefix(&origins).unwrap_or_else(|| PathBuf::from("."));
	debug!(?project_origin, "resolved common/project origin");

	let vcs_types = project::types(&project_origin)
		.await
		.into_iter()
		.filter(|pt| pt.is_vcs())
		.collect::<Vec<_>>();
	debug!(?vcs_types, "resolved vcs types");

	let (mut ignores, _errors) = ignore_files::from_origin(&project_origin).await;
	// TODO: handle errors
	debug!(?ignores, "discovered ignore files from project origin");

	let mut skip_git_global_excludes = false;
	if !vcs_types.is_empty() {
		ignores = ignores
			.into_iter()
			.filter(|ig| match ig.applies_to {
				Some(pt) if pt.is_vcs() => vcs_types.contains(&pt),
				_ => true,
			})
			.inspect(|ig| {
				if let IgnoreFile {
					applies_to: Some(ProjectType::Git),
					applies_in: None,
					..
				} = ig
				{
					warn!("project git config overrides the global excludes");
					skip_git_global_excludes = true;
				}
			})
			.collect::<Vec<_>>();
		debug!(?ignores, "filtered ignores to only those for project vcs");
		// TODO: use drain_ignore when that stabilises
	}

	let (mut global_ignores, _errors) = ignore_files::from_environment().await;
	// TODO: handle errors
	debug!(?global_ignores, "discovered ignore files from environment");

	if skip_git_global_excludes {
		global_ignores = global_ignores
			.into_iter()
			.filter(|gig| {
				!matches!(
					gig,
					IgnoreFile {
						applies_to: Some(ProjectType::Git),
						applies_in: None,
						..
					}
				)
			})
			.collect::<Vec<_>>();
		debug!(
			?global_ignores,
			"filtered global ignores to exclude global git ignores"
		);
		// TODO: use drain_ignore when that stabilises
	}

	if !vcs_types.is_empty() {
		ignores.extend(global_ignores.into_iter().filter(|ig| match ig.applies_to {
			Some(pt) if pt.is_vcs() => vcs_types.contains(&pt),
			_ => true,
		}));
		debug!(?ignores, "combined and applied final filter over ignores");
	}

	let mut filters = Vec::new();

	// TODO: move into config
	let workdir = env::current_dir()
		.and_then(|wd| wd.canonicalize())
		.into_diagnostic()?;
	for filter in args.values_of("filter").unwrap_or_default() {
		let mut filter: Filter = filter.parse()?;
		filter.in_path = Some(workdir.clone());
		filters.push(filter);
	}

	for ext in args
		.values_of("extensions")
		.unwrap_or_default()
		.map(|s| s.split(',').map(|s| s.trim()))
		.flatten()
	{
		filters.push(Filter {
			in_path: None,
			on: Matcher::Path,
			op: Op::Regex,
			pat: Pattern::Regex(Regex::new(&format!("[.]{}$", ext)).into_diagnostic()?),
			negate: false,
		});
	}

	debug!(?filters, "parsed filters and extensions");

	let (init, runtime, filterer) = config::new(&args)?;
	filterer.add_filters(&filters).await?;

	for ignore in &ignores {
		filterer.add_ignore_file(ignore).await?;
	}

	let wx = Watchexec::new(init, runtime)?;

	if !args.is_present("postpone") {
		wx.send_event(Event::default()).await?;
	}

	wx.main().await.into_diagnostic()??;

	Ok(())
}
