plugins {
    kotlin("jvm") version "2.0.21"
    application
}

repositories {
    mavenCentral()
}

dependencies {
    implementation("net.java.dev.jna:jna:5.14.0")
}

application {
    mainClass.set("io.ilink.sdk.ExampleKt")
}

// Pass the shared library location through, e.g.:
//   ./gradlew run -Dilink.jna.path=../../target/release
tasks.named<JavaExec>("run") {
    (System.getProperty("ilink.jna.path") ?: "../../target/release").let { path ->
        systemProperty("jna.library.path", path)
    }
}
