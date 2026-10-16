# Embedded native libraries

`bindings/scripts/stage_native.py --binding go` copies the shared library for a
platform into this directory:

```
native/<platform>/ilink_sdk.dll | libilink_sdk.so | libilink_sdk.dylib
```

`embed.go` embeds whatever is here. The release build stages all six platforms
before publishing, so a `go get`-ed module works with no download step. When the
directory holds only this file (a source checkout before staging), the resolver
falls back to the user cache and then to the GitHub release download.

Do not commit built libraries by hand; stage them with the script or CI.
