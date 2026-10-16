// Minimal C++ consumer of the iLink SDK.
//
// Build the library first (from the repository root): cargo build --release
// Then (Windows/MinGW):
//   g++ -std=c++17 -I include bindings/cpp/example.cpp \
//       target/release/ilink_sdk.dll -o ilink_cpp_demo.exe
//   copy target\release\ilink_sdk.dll . && ilink_cpp_demo.exe

#include <iostream>
#include <string>

#include "ilink_sdk.hpp"

int main() {
    std::cout << "version   : " << ilink::version() << "\n";
    std::cout << "mime.info : " << ilink::call("mime.info", R"({"path":"photo.PNG"})") << "\n";

    const std::string encrypted =
        ilink::call("crypto.encrypt",
                    R"({"key_hex":"00112233445566778899aabbccddeeff","data_base64":"aGVsbG8gd29ybGQ="})");
    std::cout << "encrypt   : " << encrypted << "\n";

    ilink::Client client;
    std::cout << "accounts  : " << client.accounts() << "\n";

    try {
        ilink::call("does.not.exist");
    } catch (const ilink::Error& err) {
        std::cout << "error     : kind=" << err.kind() << " message=" << err.what() << "\n";
    }
    return 0;
}
