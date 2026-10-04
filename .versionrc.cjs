// Release configuration for commit-and-tag-version.
//
// The application version is the `project(... VERSION ...)` value in
// CMakeLists.txt, which is the only place a release number is written now that
// the service is a Qt application rather than a Rust one. The old config owned
// Cargo.toml, Cargo.lock, bin/common.sh, compose.yaml, Makefile and .env.example;
// every one of those files is gone.
//
// Links point at the GitHub remote, which is where the release workflow
// publishes; the Forgejo remote runs the same commits.
const versionLine = /^(project\(lumen VERSION )(\d+\.\d+\.\d+)( LANGUAGES CXX\))$/m;

const cmakeVersion = {
  readVersion: (contents) => {
    const found = contents.match(versionLine);
    if (!found) throw new Error("no project(lumen VERSION x.y.z) line in CMakeLists.txt");
    return found[2];
  },
  writeVersion: (contents, version) => contents.replace(versionLine, `$1${version}$3`),
};

module.exports = {
  packageFiles: [{ filename: "CMakeLists.txt", updater: cmakeVersion }],
  bumpFiles: [{ filename: "CMakeLists.txt", updater: cmakeVersion }],
  tagPrefix: "v",
  releaseCommitMessageFormat: "chore(release): {{currentTag}}",
  commitUrlFormat: "https://github.com/blackopsrepl/lumen/commit/{{hash}}",
  compareUrlFormat:
    "https://github.com/blackopsrepl/lumen/compare/{{previousTag}}...{{currentTag}}",
  issueUrlFormat: "https://github.com/blackopsrepl/lumen/issues/{{id}}",
};
