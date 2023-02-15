{
  dist_version: "0.0.2",
  releases: [{
    app_name: "watchexec",
    app_version: $version,
    changelog_title: "CLI \($version)",
    changelog_body: $changelog,
    artifacts: [ $files | split("\n") | .[] | {
      name: .,
      kind: (if (. | test("[.](deb|rpm)$")) then "installer" else "executable-zip" end),
      target_triples: (. | [capture("watchexec-[^-]+-(?<target>[^.]+)[.].+").target]),
      assets: ([[
        {
          kind: "executable",
          name: (if (. | test("windows")) then "watchexec.exe" else "watchexec" end),
          path: "\(
            capture("(?<dir>watchexec-[^-]+-[^.]+)[.].+").dir
          )\(
            if (. | test("windows")) then "\\watchexec.exe" else "/watchexec" end
          )",
        },
        (if (. | test("[.](deb|rpm)$")) then null else {kind: "readme", name: "README.md"} end),
        (if (. | test("[.](deb|rpm)$")) then null else {kind: "license", name: "LICENSE"} end)
      ][] | select(. != null)])
    } ]
  }]
}
