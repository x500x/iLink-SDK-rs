# Releases & distribution

> 中文版: [中文文档](../zh/reference/distribution.md)

How the SDK is versioned, where each artifact is published, and how to cut a
release. The same information for the language bindings is summarised in
[Language bindings](bindings.md#installing-a-binding).

## Where things live

Every published artifact is produced from a single git tag (`vX.Y.Z`).

| Ecosystem | Artifact | Registry | Address |
| --- | --- | --- | --- |
| Rust | `ilink-sdk` crate | crates.io | <https://crates.io/crates/ilink-sdk> |
| Python | `ilink-sdk` wheel | PyPI | <https://pypi.org/project/ilink-sdk/> |
| Java | `io.github.x500x:ilink-sdk` | Maven Central | <https://central.sonatype.com/artifact/io.github.x500x/ilink-sdk> |
| Kotlin | `io.github.x500x:ilink-sdk-kotlin` | Maven Central | <https://central.sonatype.com/artifact/io.github.x500x/ilink-sdk-kotlin> |
| Go | `…/bindings/go/ilink` module | Go module proxy | <https://pkg.go.dev/github.com/x500x/iLink-SDK-rs/bindings/go/ilink> |
| C / C++ | native library + headers | GitHub Releases | <https://github.com/x500x/iLink-SDK-rs/releases> |

The Maven `groupId` is `io.github.x500x`, the namespace GitHub verifies
automatically. The Java package name stays `io.ilink.sdk`; the two are
independent.

GitHub Releases is the source of truth for the raw native libraries
(`ilink_sdk.dll` / `libilink_sdk.so` / `libilink_sdk.dylib`, the import/static
libraries and `ilink_sdk.h`) for six platforms: `linux-x86_64`,
`linux-aarch64`, `macos-x86_64`, `macos-aarch64`, `windows-x86_64`,
`windows-aarch64`.

## Versioning

The version lives in several manifests. They are kept in sync from the tag by
`bindings/scripts/set_version.py`:

```bash
python bindings/scripts/set_version.py --check   # verify they agree (CI does this)
python bindings/scripts/set_version.py 2.5.0     # set an explicit version
python bindings/scripts/set_version.py           # adopt Cargo.toml's version
```

The script updates `Cargo.toml`, `bindings/python/pyproject.toml`,
`bindings/python/ilink_sdk/__init__.py`, `bindings/java/pom.xml`,
`bindings/kotlin/build.gradle.kts` and `bindings/go/ilink/version.go`. CI fails
if they disagree, and the release workflow refuses to publish when the tag and
`Cargo.toml` differ.

## Cutting a release

1. Bump the version and commit:

   ```bash
   python bindings/scripts/set_version.py 2.5.0
   git commit -am "release: v2.5.0"
   ```

2. Tag and push. This triggers
   [`.github/workflows/release.yml`](../../.github/workflows/release.yml):

   ```bash
   git tag v2.5.0
   git push origin main v2.5.0
   ```

3. The workflow then, in order:

   - syncs every manifest to `2.5.0` and verifies the tag matches;
   - builds the native library for all six platforms;
   - builds a Python wheel per platform and publishes them to PyPI;
   - bundles every platform into one Java JAR and one Kotlin JAR, then deploys
     them to Maven Central;
   - publishes the crate to crates.io;
   - attaches every native archive, wheel and JAR to the GitHub release;
   - creates the `bindings/go/v2.5.0` tag that `go get` needs for the Go
     submodule.

A manual **Release → Run workflow** builds the same artifacts without
publishing; tick *publish* to push to the registries.

## Required secrets

The workflow publishes only when the matching secret is present; otherwise it
builds (and for crates.io, dry-runs) and moves on, so a fork can still run it.

| Secret | Used for |
| --- | --- |
| `CARGO_REGISTRY_TOKEN` | `cargo publish` to crates.io |
| `PYPI_API_TOKEN` | PyPI upload (omit to use trusted publishing / OIDC) |
| `MAVEN_CENTRAL_USERNAME` / `MAVEN_CENTRAL_PASSWORD` | Sonatype Central Portal |
| `MAVEN_GPG_PRIVATE_KEY` / `MAVEN_GPG_PASSPHRASE` | signing the Maven artifacts |

## Installing a released build

```bash
cargo add ilink-sdk          # Rust
pip install ilink-sdk        # Python
```

```xml
<!-- Java -->
<dependency>
  <groupId>io.github.x500x</groupId>
  <artifactId>ilink-sdk</artifactId>
  <version>2.5.0</version>
</dependency>
```

```kotlin
// Kotlin
dependencies {
    implementation("io.github.x500x:ilink-sdk-kotlin:2.5.0")
}
```

```bash
# Go (pure Go, no cgo)
go get github.com/x500x/iLink-SDK-rs/bindings/go/ilink
```

For C and C++, download the archive for your platform from the
[releases page](https://github.com/x500x/iLink-SDK-rs/releases), or install the
CMake package from a checkout (`cmake --install bindings/cpp --prefix <dir>`).
