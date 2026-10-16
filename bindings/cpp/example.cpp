// Minimal C++ consumer of the iLink SDK.
//
// Build the library first (from the repository root): cargo build --release
// Then (Windows/MinGW):
//   g++ -std=c++17 -I include -I bindings/cpp bindings/cpp/example.cpp ^
//       target/release/ilink_sdk.dll -o ilink_cpp_demo.exe
//   copy target\release\ilink_sdk.dll . && ilink_cpp_demo.exe

#include <iostream>
#include <string>

#include "ilink_sdk.hpp"

int main() {
    std::cout << "version   : " << ilink::version() << "\n";

    // Typed helpers decode the result JSON into the models in ilink_types.hpp.
    ilink::Client client;
    const ilink::MimeInfo info = client.mime_info("photo.PNG");
    std::cout << "mime.info : " << info.mime << " (image=" << info.is_image
              << ", ext=" << info.extension << ")\n";

    const ilink::CryptoResult encrypted =
        client.crypto_encrypt("00112233445566778899aabbccddeeff", "aGVsbG8gd29ybGQ=");
    std::cout << "encrypt   : " << encrypted.data_base64 << "\n";

    const std::vector<ilink::Account> accounts = client.accounts();
    std::cout << "accounts  : " << accounts.size() << "\n";

    // The raw entry points remain available for operations without a helper.
    ilink::json::Value raw = ilink::call_json("mime.extension", R"({"mime":"image/png"})");
    std::cout << "extension : " << raw["extension"].as_string() << "\n";

    try {
        ilink::call("does.not.exist");
    } catch (const ilink::Error& err) {
        std::cout << "error     : kind=" << err.kind() << " message=" << err.what() << "\n";
    }
    return 0;
}
