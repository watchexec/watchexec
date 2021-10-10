//! Detect project type and origin.

use std::{
	fs::Metadata,
	io::Error,
	path::{Path, PathBuf},
};

use futures::{future::ready as is_true, stream::FuturesUnordered, StreamExt};
use tokio::fs::metadata;
use tracing::trace;

pub async fn origins(path: impl AsRef<Path>) -> Vec<PathBuf> {
	let mut origins = Vec::new();

	async fn check_origin(path: &Path) -> bool {
		let dirtests: FuturesUnordered<_> = vec![
			dir_exists(path.join("_darcs")),
			dir_exists(path.join(".bzr")),
			dir_exists(path.join(".fossil-settings")),
			dir_exists(path.join(".git")),
			dir_exists(path.join(".github")),
			dir_exists(path.join(".hg")),
		]
		.into_iter()
		.collect();

		let filetests: FuturesUnordered<_> = vec![
			file_exists(path.join(".asf.yaml")),
			file_exists(path.join(".bzrignore")),
			file_exists(path.join(".codecov.yml")),
			file_exists(path.join(".ctags")),
			file_exists(path.join(".editorconfig")),
			file_exists(path.join(".gitattributes")),
			file_exists(path.join(".gitmodules")),
			file_exists(path.join(".hgignore")),
			file_exists(path.join(".hgtags")),
			file_exists(path.join(".perltidyrc")),
			file_exists(path.join(".travis.yml")),
			file_exists(path.join("appveyor.yml")),
			file_exists(path.join("build.gradle")),
			file_exists(path.join("build.properties")),
			file_exists(path.join("build.xml")),
			file_exists(path.join("Cargo.toml")),
			file_exists(path.join("cgmanifest.json")),
			file_exists(path.join("CMakeLists.txt")),
			file_exists(path.join("composer.json")),
			file_exists(path.join("COPYING")),
			file_exists(path.join("docker-compose.yml")),
			file_exists(path.join("Dockerfile")),
			file_exists(path.join("Gemfile")),
			file_exists(path.join("LICENSE.txt")),
			file_exists(path.join("LICENSE")),
			file_exists(path.join("Makefile.am")),
			file_exists(path.join("Makefile.pl")),
			file_exists(path.join("Makefile.PL")),
			file_exists(path.join("Makefile")),
			file_exists(path.join("mix.exs")),
			file_exists(path.join("moonshine-dependencies.xml")),
			file_exists(path.join("package.json")),
			file_exists(path.join("pom.xml")),
			file_exists(path.join("project.clj")),
			file_exists(path.join("README.md")),
			file_exists(path.join("README")),
			file_exists(path.join("requirements.txt")),
			file_exists(path.join("v.mod")),
		]
		.into_iter()
		.collect();

		dirtests.any(is_true).await || filetests.any(is_true).await
	}

	let mut current = path.as_ref();
	if check_origin(path.as_ref()).await {
		origins.push(current.to_owned());
	}

	while let Some(parent) = current.parent() {
		current = parent;
		if check_origin(current).await {
			origins.push(current.to_owned());
			continue;
		}
	}

	origins
}

/// Returns all project types detected at this given origin.
///
/// This should be called with a result of [`origins()`], or a project origin if already known; it
/// will not find the origin itself.
pub async fn types(path: impl AsRef<Path>) -> Result<Vec<ProjectType>, Error> {
	todo!()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ProjectType {
	Bazaar,
	Darcs,
	Fossil,
	Git,
	Mercurial,
	Pijul,

	Bundler,
	Cargo,
	JavaScript,
	Pip,
	RubyGem,
}

#[inline]
async fn exists(path: &Path) -> Option<Metadata> {
	metadata(path).await.ok()
}

#[inline]
async fn file_exists(path: PathBuf) -> bool {
	let res = exists(&path)
		.await
		.map(|meta| meta.is_file())
		.unwrap_or(false);

	if res {
		trace!(?path, "file exists");
	}

	res
}

#[inline]
async fn dir_exists(path: PathBuf) -> bool {
	let res = exists(&path)
		.await
		.map(|meta| meta.is_dir())
		.unwrap_or(false);

	if res {
		trace!(?path, "dir exists");
	}

	res
}
