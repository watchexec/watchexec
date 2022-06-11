//! Detect project type and origin.

use std::{
	collections::{HashMap, HashSet},
	fs::FileType,
	path::{Path, PathBuf},
};

use futures::StreamExt;
use tokio::fs::read_dir;
use tokio_stream::wrappers::ReadDirStream;

/// Project types recognised by watchexec.
///
/// There are two kinds of projects: VCS and software suite. The latter is more characterised by
/// what package manager or build system is in use. The enum is marked non-exhaustive as more types
/// can get added in the future.
///
/// Do not rely on the ordering or value (e.g. with transmute) of the variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ProjectType {
	/// VCS: [Bazaar](https://bazaar.canonical.com/).
	Bazaar,

	/// VCS: [Darcs](http://darcs.net/).
	Darcs,

	/// VCS: [Fossil](https://www.fossil-scm.org/).
	Fossil,

	/// VCS: [Git](https://git-scm.com/).
	Git,

	/// VCS: [Mercurial](https://www.mercurial-scm.org/).
	Mercurial,

	/// VCS: [Pijul](https://pijul.org/).
	Pijul,

	/// VCS: [Subversion](https://subversion.apache.org) (aka SVN).
	Subversion,

	/// Soft: [Ruby](https://www.ruby-lang.org/)’s [Bundler](https://bundler.io/).
	Bundler,

	/// Soft: the [C programming language](https://en.wikipedia.org/wiki/C_(programming_language)).
	C,

	/// Soft: [Rust](https://www.rust-lang.org/)’s [Cargo](https://doc.rust-lang.org/cargo/).
	Cargo,

	/// Soft: the [Docker](https://www.docker.com/) container runtime.
	Docker,

	/// Soft: the [Elixir](https://elixir-lang.org/) language.
	Elixir,

	/// Soft: [Java](https://www.java.com/)’s [Gradle](https://gradle.org/).
	Gradle,

	/// Soft: [EcmaScript](https://www.ecmascript.org/) (aka JavaScript).
	///
	/// This is a catch-all for all `package.json`-based projects.
	JavaScript,

	/// Soft: [Clojure](https://clojure.org/)’s [Leiningen](https://leiningen.org/).
	Leiningen,

	/// Soft: [Java](https://www.java.com/)’s [Maven](https://maven.apache.org/).
	Maven,

	/// Soft: the [Perl](https://www.perl.org/) language.
	Perl,

	/// Soft: the [PHP](https://www.php.net/) language.
	PHP,

	/// Soft: [Python](https://www.python.org/)’s [Pip](https://www.pip.org/).
	Pip,

	/// Soft: the [V](https://www.v-lang.org/) language.
	V,
}

impl ProjectType {
	/// Returns true if the project type is a VCS.
	pub fn is_vcs(self) -> bool {
		matches!(
			self,
			Self::Bazaar
				| Self::Darcs | Self::Fossil
				| Self::Git | Self::Mercurial
				| Self::Pijul | Self::Subversion
		)
	}

	/// Returns true if the project type is a software suite.
	pub fn is_soft(self) -> bool {
		matches!(
			self,
			Self::Bundler
				| Self::C | Self::Cargo
				| Self::Docker | Self::Elixir
				| Self::Gradle | Self::JavaScript
				| Self::Leiningen
				| Self::Maven | Self::Perl
				| Self::PHP | Self::Pip
				| Self::V
		)
	}
}

/// Traverses the parents of the given path and returns _all_ that are project origins.
///
/// This checks for the presence of a wide range of files and directories that are likely to be
/// present and indicative of the root or origin path of a project. It's entirely possible to have
/// multiple such origins show up: for example, a member of a Cargo workspace will list both the
/// member project and the workspace root as origins.
pub async fn origins(path: impl AsRef<Path>) -> HashSet<PathBuf> {
	let mut origins = HashSet::new();

	fn check_list(list: DirList) -> bool {
		if list.is_empty() {
			return false;
		}

		[
			list.has_dir("_darcs"),
			list.has_dir(".bzr"),
			list.has_dir(".fossil-settings"),
			list.has_dir(".git"),
			list.has_dir(".github"),
			list.has_dir(".hg"),
			list.has_dir(".svn"),
			list.has_file(".asf.yaml"),
			list.has_file(".bzrignore"),
			list.has_file(".codecov.yml"),
			list.has_file(".ctags"),
			list.has_file(".editorconfig"),
			list.has_file(".gitattributes"),
			list.has_file(".gitmodules"),
			list.has_file(".hgignore"),
			list.has_file(".hgtags"),
			list.has_file(".perltidyrc"),
			list.has_file(".travis.yml"),
			list.has_file("appveyor.yml"),
			list.has_file("build.gradle"),
			list.has_file("build.properties"),
			list.has_file("build.xml"),
			list.has_file("Cargo.toml"),
			list.has_file("Cargo.lock"),
			list.has_file("cgmanifest.json"),
			list.has_file("CMakeLists.txt"),
			list.has_file("composer.json"),
			list.has_file("COPYING"),
			list.has_file("docker-compose.yml"),
			list.has_file("Dockerfile"),
			list.has_file("Gemfile"),
			list.has_file("LICENSE.txt"),
			list.has_file("LICENSE"),
			list.has_file("Makefile.am"),
			list.has_file("Makefile.pl"),
			list.has_file("Makefile.PL"),
			list.has_file("Makefile"),
			list.has_file("mix.exs"),
			list.has_file("moonshine-dependencies.xml"),
			list.has_file("package.json"),
			list.has_file("pom.xml"),
			list.has_file("project.clj"),
			list.has_file("README.md"),
			list.has_file("README"),
			list.has_file("requirements.txt"),
			list.has_file("v.mod"),
		]
		.into_iter()
		.any(|f| f)
	}

	let mut current = path.as_ref();
	if check_list(DirList::obtain(current).await) {
		origins.insert(current.to_owned());
	}

	while let Some(parent) = current.parent() {
		current = parent;
		if check_list(DirList::obtain(current).await) {
			origins.insert(current.to_owned());
			continue;
		}
	}

	origins
}

/// Returns all project types detected at this given origin.
///
/// This should be called with a result of [`origins()`], or a project origin if already known; it
/// will not find the origin itself.
///
/// The returned list may be empty.
///
/// Note that this only detects project types listed in the [`ProjectType`] enum, and may not detect
/// anything for some paths returned by [`origins()`].
pub async fn types(path: impl AsRef<Path>) -> HashSet<ProjectType> {
	let list = DirList::obtain(path.as_ref()).await;
	[
		list.if_has_dir("_darcs", ProjectType::Darcs),
		list.if_has_dir(".bzr", ProjectType::Bazaar),
		list.if_has_dir(".fossil-settings", ProjectType::Fossil),
		list.if_has_dir(".git", ProjectType::Git),
		list.if_has_dir(".hg", ProjectType::Mercurial),
		list.if_has_dir(".svn", ProjectType::Subversion),
		list.if_has_file(".bzrignore", ProjectType::Bazaar),
		list.if_has_file(".ctags", ProjectType::C),
		list.if_has_file(".gitattributes", ProjectType::Git),
		list.if_has_file(".gitmodules", ProjectType::Git),
		list.if_has_file(".hgignore", ProjectType::Mercurial),
		list.if_has_file(".hgtags", ProjectType::Mercurial),
		list.if_has_file(".perltidyrc", ProjectType::Perl),
		list.if_has_file("build.gradle", ProjectType::Gradle),
		list.if_has_file("Cargo.toml", ProjectType::Cargo),
		list.if_has_file("cgmanifest.json", ProjectType::JavaScript),
		list.if_has_file("composer.json", ProjectType::PHP),
		list.if_has_file("Dockerfile", ProjectType::Docker),
		list.if_has_file("Gemfile", ProjectType::Bundler),
		list.if_has_file("Makefile.PL", ProjectType::Perl),
		list.if_has_file("mix.exs", ProjectType::Elixir),
		list.if_has_file("package.json", ProjectType::JavaScript),
		list.if_has_file("pom.xml", ProjectType::Maven),
		list.if_has_file("project.clj", ProjectType::Leiningen),
		list.if_has_file("requirements.txt", ProjectType::Pip),
		list.if_has_file("v.mod", ProjectType::V),
	]
	.into_iter()
	.flatten()
	.collect()
}

#[derive(Debug, Default)]
struct DirList(HashMap<PathBuf, FileType>);
impl DirList {
	async fn obtain(path: &Path) -> Self {
		if let Ok(s) = read_dir(path).await {
			Self(
				ReadDirStream::new(s)
					.filter_map(|entry| async move {
						match entry {
							Err(_) => None,
							Ok(entry) => {
								if let (Ok(path), Ok(file_type)) =
									(entry.path().strip_prefix(path), entry.file_type().await)
								{
									Some((path.to_owned(), file_type))
								} else {
									None
								}
							}
						}
					})
					.collect::<HashMap<_, _>>()
					.await,
			)
		} else {
			Self::default()
		}
	}

	#[inline]
	fn is_empty(&self) -> bool {
		self.0.is_empty()
	}

	#[inline]
	fn has_file(&self, name: impl AsRef<Path>) -> bool {
		let name = name.as_ref();
		self.0.get(name).map(|x| x.is_file()).unwrap_or(false)
	}

	#[inline]
	fn has_dir(&self, name: impl AsRef<Path>) -> bool {
		let name = name.as_ref();
		self.0.get(name).map(|x| x.is_dir()).unwrap_or(false)
	}

	#[inline]
	fn if_has_file(&self, name: impl AsRef<Path>, project: ProjectType) -> Option<ProjectType> {
		if self.has_file(name) {
			Some(project)
		} else {
			None
		}
	}

	#[inline]
	fn if_has_dir(&self, name: impl AsRef<Path>, project: ProjectType) -> Option<ProjectType> {
		if self.has_dir(name) {
			Some(project)
		} else {
			None
		}
	}
}
