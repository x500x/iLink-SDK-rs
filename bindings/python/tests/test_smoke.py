"""Smoke tests for the Python bindings. Run directly or with pytest."""

import ilink_sdk


def test_version():
    assert ilink_sdk.version().count(".") == 2


def test_pure_helper():
    info = ilink_sdk.call("mime.info", {"path": "photo.PNG"})
    assert info["mime"] == "image/png"
    assert info["is_image"] is True


def test_unknown_op_raises_with_kind():
    try:
        ilink_sdk.call("does.not.exist")
    except ilink_sdk.IlinkError as err:
        assert err.kind == "unknown"
        assert "unknown op" in err.message
    else:  # pragma: no cover
        raise AssertionError("expected IlinkError")


def test_crypto_round_trip():
    key = "00112233445566778899aabbccddeeff"
    enc = ilink_sdk.call("crypto.encrypt", {"key_hex": key, "data_base64": "aGVsbG8gd29ybGQ="})
    dec = ilink_sdk.call("crypto.decrypt", {"key_hex": key, "data_base64": enc["data_base64"]})
    import base64

    assert base64.b64decode(dec["data_base64"]) == b"hello world"


def test_client_routing_merge():
    client = ilink_sdk.Client(account_id="demo")
    # session.status only needs an account id; the client injects it.
    assert client.session_status()["paused"] is False


if __name__ == "__main__":
    for name, fn in sorted(globals().items()):
        if name.startswith("test_") and callable(fn):
            fn()
            print(f"ok  {name}")
