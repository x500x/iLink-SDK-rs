plugins {
    kotlin("jvm") version "2.0.21"
    `java-library`
    `maven-publish`
    signing
    application
    // Publishes the staged bundle through the Central Portal publisher API.
    // The settings plugin (settings.gradle.kts) supplies the version and the
    // credentials.
    id("com.gradleup.nmcp")
}

group = "io.github.x500x"
version = "2.4.10"

repositories {
    mavenCentral()
}

dependencies {
    api("net.java.dev.jna:jna:5.14.0")
}

java {
    // The JAR bundles the native library, so it is platform-specific even though
    // the bytecode targets a fixed Java version.
    withSourcesJar()
}

application {
    mainClass.set("io.ilink.sdk.ExampleKt")
}

// Pass the shared library location through, e.g.:
//   ./gradlew run -Dilink.jna.path=../../target/release
// The published JAR does not need this: NativeLoader extracts the library that
// bindings/scripts/stage_native.py staged into src/main/resources.
tasks.named<JavaExec>("run") {
    (System.getProperty("ilink.jna.path") ?: "../../target/release").let { path ->
        systemProperty("jna.library.path", path)
    }
}

publishing {
    publications {
        create<MavenPublication>("maven") {
            from(components["java"])
            artifactId = "ilink-sdk-kotlin"
            pom {
                name.set("iLink SDK (Kotlin)")
                description.set(
                    "Kotlin bindings for the Rust iLink (Weixin) SDK, with the native library bundled",
                )
                url.set("https://github.com/x500x/iLink-SDK-rs")
                licenses {
                    license {
                        name.set("MIT")
                        url.set("https://opensource.org/licenses/MIT")
                    }
                }
                developers {
                    developer {
                        name.set("iLink SDK contributors")
                        organizationUrl.set("https://github.com/x500x/iLink-SDK-rs")
                    }
                }
                scm {
                    connection.set("scm:git:https://github.com/x500x/iLink-SDK-rs.git")
                    url.set("https://github.com/x500x/iLink-SDK-rs")
                }
            }
        }
    }
    repositories {
        // Snapshots only. Releases go through the Central Portal publisher API
        // (nmcp), because a plain PUT to the maven-releases repository returns
        // 404 -- see settings.gradle.kts.
        maven {
            name = "centralSnapshots"
            url = uri("https://central.sonatype.com/repository/maven-snapshots/")
            credentials {
                username = (findProperty("centralUsername") as String?) ?: ""
                password = (findProperty("centralPassword") as String?) ?: ""
            }
        }
    }
}

// Signing is enabled only when the release workflow supplies a key, so a plain
// `./gradlew build` works without one.
signing {
    val key = System.getenv("GPG_SIGNING_KEY")
    val password = System.getenv("GPG_SIGNING_PASSWORD")
    isRequired = key != null
    if (key != null) {
        useInMemoryPgpKeys(key, password)
        sign(publishing.publications)
    }
}
