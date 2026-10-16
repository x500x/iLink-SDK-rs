"""Smoke tests for the Python bindings. Run directly or with pytest."""

import ilink_sdk


def test_version():
    assert ilink_sdk.version().count(".") == 2


def test_pure_helper():
    info = ilink_sdk.call("mime.info", {"path": "photo.PNG"})
    assert info["mime"] == "image/png"
    assert info["is_image"] is True


def test_typed_mime_info():
    info = ilink_sdk.Client().mime_info("photo.PNG")
    assert isinstance(info, ilink_sdk.models.MimeInfo)
    assert info.mime == "image/png"
    assert info.is_image is True
    assert info.extension == ".png"


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
    client = ilink_sdk.Client()
    enc = client.crypto_encrypt(key, b"hello world")
    assert enc.padded_size == 16
    assert client.crypto_decrypt(key, enc.data_base64) == b"hello world"


def test_client_routing_merge():
    client = ilink_sdk.Client(account_id="demo")
    # session.status only needs an account id; the client injects it.
    status = client.session_status()
    assert isinstance(status, ilink_sdk.models.SessionStatus)
    assert status.paused is False


if __name__ == "__main__":
    for name, fn in sorted(globals().items()):
        if name.startswith("test_") and callable(fn):
            fn()
            print(f"ok  {name}")
