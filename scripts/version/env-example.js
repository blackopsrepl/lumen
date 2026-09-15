// commit-and-tag-version updater: .env.example carries the version twice — the
// LUMEN_VERSION value and the commented image tag — and both have to move
// together with the release.
const VALUE = /^LUMEN_VERSION=([0-9][0-9A-Za-z.-]*)/m;
const IMAGE = /localhost\/lumen:([0-9][0-9A-Za-z.-]*)/g;

module.exports = {
  readVersion(contents) {
    const match = contents.match(VALUE);
    return match ? match[1] : null;
  },
  writeVersion(contents, version) {
    return contents
      .replace(VALUE, () => `LUMEN_VERSION=${version}`)
      .replace(IMAGE, () => `localhost/lumen:${version}`);
  },
};
