# Bundled native libraries

`bindings/scripts/stage_native.py --binding kotlin` copies the shared library for a
platform into this directory:

```
io/ilink/sdk/native/<platform>/ilink_sdk.dll | libilink_sdk.so | libilink_sdk.dylib
```

`NativeLoader` picks the entry for the running OS/CPU and extracts it at runtime,
so the JAR works on every bundled platform with no manual setup.

The release build stages all six platforms before `mvn package`, so a published
JAR is self-contained. Do not commit built libraries by hand; stage them with the
script or CI.
