//go:build !ilink_cgo

package ilink

// sdkVersion is the iLink SDK release these bindings target. It selects the
// matching GitHub release when the native library has to be downloaded, and
// namespaces the user cache directory. Keep it in sync with Cargo.toml.
const sdkVersion = "2.4.11"

// repoSlug is the GitHub repository that publishes the native release assets.
const repoSlug = "x500x/iLink-SDK-rs"

// libraryVersion returns the version used for cache namespacing.
func libraryVersion() string { return sdkVersion }
