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
| Java | `io.github.x500x:ilink-sdk` | Maven Central | <https://central.sonatype.com/artifact/io.github.x500x/ilink-sdk> |
| Kotlin | `io.github.x500x:ilink-sdk-kotlin` | Maven Central | <https://central.sonatype.com/artifact/io.github.x500x/ilink-sdk-kotlin> |
| Go | `…/bindings/go/ilink` 模块 | Go module proxy | <https://pkg.go.dev/github.com/x500x/iLink-SDK-rs/bindings/go/ilink> |
| C / C++ | 原生库 + 头文件 | GitHub Releases | <https://github.com/x500x/iLink-SDK-rs/releases> |

Maven 的 `groupId` 用 `io.github.x500x`，这是 GitHub 自动验证的命名空间。Java 包名仍为
`io.ilink.sdk`，两者互不相关。

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

注册表失败不会阻断 GitHub Release：PyPI、crates.io、Maven Central 的步骤都是
`continue-on-error`，因此即使某个仓库拒绝了上传，Release 仍会产出原生库压缩包（Go 的
下载回退依赖它）。

## 各仓库的一次性配置

在支持的情况下，流程不使用长期 token 进行认证。下面每一项都是**只能由仓库所有者本人
在浏览器中完成**的一次性操作——它是仓库用来证明你拥有该项目的方式。

| 仓库 | 一次性操作 |
| --- | --- |
| PyPI | 在 <https://pypi.org/manage/account/publishing/> 添加 trusted publisher：项目 `ilink-sdk`、owner `x500x`、repo `iLink-SDK-rs`、workflow `release.yml`、environment *（留空）*。无需 secret。 |
| crates.io | 在 <https://crates.io/settings/tokens> → *Trusted Publishing* 添加：owner `x500x`、repo `iLink-SDK-rs`、workflow `release.yml`。无需 secret。 |
| Maven Central | 注册 Sonatype Central Portal 账号，并验证 `io.github.x500x` 命名空间（GitHub 自动验证）。 |
| Maven Central | 登记发布用 GPG 公钥（见下）。 |

### Secrets

流程会读取以下 GitHub Actions secrets。缺失时降级为只构建而不报错，因此 fork 也能跑通。

| Secret | 用途 | 状态 |
| --- | --- | --- |
| `MAVEN_GPG_PRIVATE_KEY` | 为 Maven 产物签名 | **已配置** |
| `MAVEN_GPG_PASSPHRASE` | 该密钥的口令 | **已配置** |
| `MAVEN_CENTRAL_USERNAME` / `MAVEN_CENTRAL_PASSWORD` | Sonatype Central Portal | 拿到账号后需要 |

**不需要** `CARGO_REGISTRY_TOKEN` 和 `PYPI_API_TOKEN`：crates.io 与 PyPI 使用 OIDC
可信发布。

### 发布签名密钥

本仓库已配置一把 4096 位 RSA 签名密钥，其公钥已发布到 `keyserver.ubuntu.com`：

```bash
# 指纹
gpg --keyserver hkps://keyserver.ubuntu.com --recv-keys \
    FC00B490E76352FFF0B84C6E0B170264E9B37831
```

Maven Central 会向公钥服务器验证签名，这正是签名上传能被接受的原因。私钥与口令只存在于
上面两个 secret 中。

## 安装已发布版本

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
# Go（纯 Go，无需 cgo）
go get github.com/x500x/iLink-SDK-rs/bindings/go/ilink
```

C 与 C++ 请从
[releases 页面](https://github.com/x500x/iLink-SDK-rs/releases) 下载对应平台的压缩包，
或在检出目录中安装 CMake 包（`cmake --install bindings/cpp --prefix <dir>`）。
