use clap::Parser;

include!(env!("BOSION_PATH"));

#[derive(Parser)]
#[clap(version, long_version = Bosion::LONG_VERSION)]
struct Args {
	#[clap(long)]
	extras: bool,

	#[clap(long)]
	features: bool,

	#[clap(long)]
	dates: bool,
}

fn main() {
	let args = Args::parse();

	if args.extras {
		println!(
			"{}",
			Bosion::long_version_with(&[("extra", "field"), ("custom", "1.2.3"),])
		);
	} else

	if args.features {
		println!("Features: {}", Bosion::CRATE_FEATURE_STRING);
	} else

	if args.dates {
		println!("commit date: {}", Bosion::GIT_COMMIT_DATE);
		println!("commit datetime: {}", Bosion::GIT_COMMIT_DATETIME);
		println!("build date: {}", Bosion::BUILD_DATE);
		println!("build datetime: {}", Bosion::BUILD_DATETIME);
	} else {
		println!("{}", Bosion::LONG_VERSION);
	}
}
