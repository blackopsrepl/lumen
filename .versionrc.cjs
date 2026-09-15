// Release configuration for commit-and-tag-version.
//
// The service version is the [package] version in Cargo.toml; every other place
// a release number is written is declared here so the tool owns all of them and
// nothing drifts:
//
//   Cargo.toml          the service version (read and written)
//   Cargo.lock          the same version, so `cargo build --locked` is happy
//   bin/common.sh       LUMEN_VERSION default that tags the image
//   compose.yaml        the same default for `podman compose`
//   Makefile            the same default for `make clean`
//   .env.example        the value users copy, and the commented image name
//
// Links point at the GitHub remote, which is where the release workflow
// publishes; the Forgejo remote runs the same commits.
module.exports = {
  packageFiles: [
    { filename: "Cargo.toml", updater: "scripts/version/cargo-toml.js" },
  ],
  bumpFiles: [
    { filename: "Cargo.toml", updater: "scripts/version/cargo-toml.js" },
    { filename: "Cargo.lock", updater: "scripts/version/cargo-lock.js" },
    { filename: "bin/common.sh", updater: "scripts/version/shell-version-default.js" },
    { filename: "compose.yaml", updater: "scripts/version/shell-version-default.js" },
    { filename: "Makefile", updater: "scripts/version/shell-version-default.js" },
    { filename: ".env.example", updater: "scripts/version/env-example.js" },
  ],
  tagPrefix: "v",
  releaseCommitMessageFormat: "chore(release): {{currentTag}}",
  commitUrlFormat: "https://github.com/blackopsrepl/lumen/commit/{{hash}}",
  compareUrlFormat:
    "https://github.com/blackopsrepl/lumen/compare/{{previousTag}}...{{currentTag}}",
  issueUrlFormat: "https://github.com/blackopsrepl/lumen/issues/{{id}}",
};
