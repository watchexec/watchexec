Sometimes it's needed to release every crate in the workspace, or several dependent crates at once.
In those cases, this is the order to do it in:

- project-origins
- ignored-files (depends on project-origins)
- lib (depends on project-origins and ignored-files)
- filterer/ignore (depends on lib)
- filterer/globset and /tagged (depend on lib and filterer/ignore)
- cli (depends on everything)
