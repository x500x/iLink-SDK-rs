plugins {
    kotlin("jvm") version "2.0.21"
    `java-library`
    `maven-publish`
    application
}

group = "io.ilink"
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
                scm {
                    connection.set("scm:git:https://github.com/x500x/iLink-SDK-rs.git")
                    url.set("https://github.com/x500x/iLink-SDK-rs")
                }
            }
        }
    }
}
