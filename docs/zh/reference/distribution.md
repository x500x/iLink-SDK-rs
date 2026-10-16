# 发布与分发

> English: [English](../reference/distribution.md)

本文说明 SDK 的版本管理方式、各产物发布到哪里，以及如何发版。各语言绑定的相同信息
汇总在 [多语言绑定](bindings.md#安装绑定)。

## 产物位置

所有已发布产物都来自同一个 git tag（`vX.Y.Z`）。

| 生态 | 产物 | 仓库 | 地址 |
| --- | --- | --- | --- |
| Rust | `ilink-sdk` crate | crates.io | <https://crates.io/crates/ilink-sdk> |
| Python | `ilink-sdk` wheel | PyPI | <https://pypi.org/project/ilink-sdk/> |
| Java | `io.ilink:ilink-sdk` | Maven Central | <https://central.sonatype.com/artifact/io.ilink/ilink-sdk> |
| Kotlin | `io.ilink:ilink-sdk-kotlin` | Maven Central | <https://central.sonatype.com/artifact/io.ilink/ilink-sdk-kotlin> |
| Go | `…/bindings/go/ilink` 模块 | Go module proxy | <https://pkg.go.dev/github.com/x500x/iLink-SDK-rs/bindings/go/ilink> |
| C / C++ | 原生库 + 头文件 | GitHub Releases | <https://github.com/x500x/iLink-SDK-rs/releases> |

原生库（`ilink_sdk.dll` / `libilink_sdk.so` / `libilink_sdk.dylib`、导入库/静态库以及
`ilink_sdk.h`）以 GitHub Releases 为准，覆盖六个平台：`linux-x86_64`、
`linux-aarch64`、`macos-x86_64`、`macos-aarch64`、`windows-x86_64`、
`windows-aarch64`。

## 版本管理

版本号分散在多个清单文件中，由 `bindings/scripts/set_version.py` 依据 tag 保持同步：

```bash
python bindings/scripts/set_version.py --check   # 校验是否一致（CI 会执行）
python bindings/scripts/set_version.py 2.5.0     # 指定版本号
python bindings/scripts/set_version.py           # 采用 Cargo.toml 中的版本
```

该脚本会更新 `Cargo.toml`、`bindings/python/pyproject.toml`、
`bindings/python/ilink_sdk/__init__.py`、`bindings/java/pom.xml`、
`bindings/kotlin/build.gradle.kts` 和 `bindings/go/ilink/version.go`。CI 在不一致时会
失败；发布流程在 tag 与 `Cargo.toml` 不符时会拒绝发布。

## 发版步骤

1. 更新版本号并提交：

   ```bash
   python bindings/scripts/set_version.py 2.5.0
   git commit -am "release: v2.5.0"
   ```

2. 打 tag 并推送，触发
   [`.github/workflows/release.yml`](../../../.github/workflows/release.yml)：

   ```bash
   git tag v2.5.0
   git push origin main v2.5.0
   ```

3. 流程随后依次：

   - 把所有清单同步为 `2.5.0` 并校验 tag 一致；
   - 为六个平台构建原生库；
   - 为各平台构建 Python wheel 并发布到 PyPI；
   - 把各平台打进同一个 Java JAR 与 Kotlin JAR，部署到 Maven Central；
   - 把 crate 发布到 crates.io；
   - 把全部原生包、wheel、JAR 附加到 GitHub Release；
   - 创建 `go get` 所需的 `bindings/go/v2.5.0` 标签。

手动执行 **Release → Run workflow** 会构建同样的产物但不发布；勾选 *publish* 才会推送到
各仓库。

## 需要的 Secrets

只有当对应 secret 存在时流程才会发布；否则只构建（crates.io 则执行 dry-run）后继续，
因此 fork 也能跑通。

| Secret | 用途 |
| --- | --- |
| `CARGO_REGISTRY_TOKEN` | `cargo publish` 到 crates.io |
| `PYPI_API_TOKEN` | 上传到 PyPI（省略则用 trusted publishing / OIDC） |
| `MAVEN_CENTRAL_USERNAME` / `MAVEN_CENTRAL_PASSWORD` | Sonatype Central Portal |
| `MAVEN_GPG_PRIVATE_KEY` / `MAVEN_GPG_PASSPHRASE` | 为 Maven 产物签名 |

## 安装已发布版本

```bash
cargo add ilink-sdk          # Rust
pip install ilink-sdk        # Python
```

```xml
<!-- Java -->
<dependency>
  <groupId>io.ilink</groupId>
  <artifactId>ilink-sdk</artifactId>
  <version>2.5.0</version>
</dependency>
```

```kotlin
// Kotlin
dependencies {
    implementation("io.ilink:ilink-sdk-kotlin:2.5.0")
}
```

```bash
# Go（纯 Go，无需 cgo）
go get github.com/x500x/iLink-SDK-rs/bindings/go/ilink
```

C 与 C++ 请从
[releases 页面](https://github.com/x500x/iLink-SDK-rs/releases) 下载对应平台的压缩包，
或在检出目录中安装 CMake 包（`cmake --install bindings/cpp --prefix <dir>`）。
