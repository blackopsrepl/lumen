// commit-and-tag-version updater: the `${LUMEN_VERSION:-X.Y.Z}` defaults that
// tag the container image, in bin/common.sh, compose.yaml, and the Makefile.
// All three spell the same interpolation, so one regex covers them.
const DEFAULT = /LUMEN_VERSION:-([0-9][0-9A-Za-z.-]*)/;

module.exports = {
  readVersion(contents) {
    const match = contents.match(DEFAULT);
    return match ? match[1] : null;
  },
  writeVersion(contents, version) {
    return contents.replace(new RegExp(DEFAULT, "g"), () => `LUMEN_VERSION:-${version}`);
  },
};
