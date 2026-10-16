// ilink_json.hpp — a tiny, dependency-free JSON value used by the C++ binding.
//
// The iLink C ABI speaks JSON in and JSON out. Rather than force every C++
// caller to pull in a JSON library, this header provides just enough of a
// parser/serialiser to decode operation results into the typed models in
// `ilink_types.hpp`, and to re-encode nested objects for advanced calls.
//
// It is intentionally small: it supports the JSON the SDK emits (objects,
// arrays, strings, numbers, booleans, null) and is not a general-purpose
// replacement for a full library.

#pragma once

#include <cstdint>
#include <cstdio>
#include <map>
#include <stdexcept>
#include <string>
#include <type_traits>
#include <utility>
#include <vector>

namespace ilink {
namespace json {

/// A parsed JSON value.
class Value {
public:
    enum class Type { Null, Bool, Number, String, Array, Object };

    Value() = default;
    Value(bool b) : type_(Type::Bool), bool_(b) {}
    /// Any arithmetic type (other than bool) becomes a JSON number.
    template <typename T,
              typename = std::enable_if_t<std::is_arithmetic_v<T> && !std::is_same_v<T, bool>>>
    Value(T n) : type_(Type::Number), number_(static_cast<double>(n)) {}
    Value(std::string s) : type_(Type::String), string_(std::move(s)) {}
    Value(const char* s) : type_(Type::String), string_(s) {}

    static Value array() { Value v; v.type_ = Type::Array; return v; }
    static Value object() { Value v; v.type_ = Type::Object; return v; }

    Type type() const noexcept { return type_; }
    bool is_null() const noexcept { return type_ == Type::Null; }
    bool is_bool() const noexcept { return type_ == Type::Bool; }
    bool is_number() const noexcept { return type_ == Type::Number; }
    bool is_string() const noexcept { return type_ == Type::String; }
    bool is_array() const noexcept { return type_ == Type::Array; }
    bool is_object() const noexcept { return type_ == Type::Object; }

    // -- scalar access ----------------------------------------------------
    bool as_bool(bool fallback = false) const {
        if (type_ == Type::Bool) return bool_;
        if (type_ == Type::Number) return number_ != 0;
        return fallback;
    }

    double as_double(double fallback = 0) const {
        return type_ == Type::Number ? number_ : fallback;
    }

    std::int64_t as_int(std::int64_t fallback = 0) const {
        return type_ == Type::Number ? static_cast<std::int64_t>(number_) : fallback;
    }

    std::string as_string(const std::string& fallback = "") const {
        return type_ == Type::String ? string_ : fallback;
    }

    /// True when a value of any scalar type is present (not null).
    bool has_value() const noexcept { return type_ != Type::Null; }

    // -- array access -----------------------------------------------------
    std::size_t size() const noexcept {
        if (type_ == Type::Array) return array_.size();
        if (type_ == Type::Object) return object_.size();
        return 0;
    }

    const Value& operator[](std::size_t index) const {
        static const Value empty;
        if (type_ != Type::Array || index >= array_.size()) return empty;
        return array_[index];
    }

    void push(Value value) {
        type_ = Type::Array;
        array_.push_back(std::move(value));
    }

    // -- object access ----------------------------------------------------
    /// Returns the value for `key`, or a shared null value when absent.
    const Value& operator[](const std::string& key) const {
        static const Value empty;
        if (type_ != Type::Object) return empty;
        for (const auto& entry : object_) {
            if (entry.first == key) return entry.second;
        }
        return empty;
    }

    /// True when `key` is present and not null.
    bool contains(const std::string& key) const {
        const Value& v = (*this)[key];
        return !v.is_null();
    }

    void set(const std::string& key, Value value) {
        type_ = Type::Object;
        for (auto& entry : object_) {
            if (entry.first == key) {
                entry.second = std::move(value);
                return;
            }
        }
        object_.emplace_back(key, std::move(value));
    }

    /// Serialise back to compact JSON.
    std::string dump() const {
        std::string out;
        write(out);
        return out;
    }

private:
    void write(std::string& out) const {
        switch (type_) {
            case Type::Null: out += "null"; break;
            case Type::Bool: out += bool_ ? "true" : "false"; break;
            case Type::Number: {
                // Integers print without a trailing ".0".
                const double rounded = static_cast<double>(static_cast<std::int64_t>(number_));
                if (number_ == rounded) {
                    out += std::to_string(static_cast<std::int64_t>(number_));
                } else {
                    out += std::to_string(number_);
                }
                break;
            }
            case Type::String: write_string(out, string_); break;
            case Type::Array: {
                out += '[';
                for (std::size_t i = 0; i < array_.size(); ++i) {
                    if (i) out += ',';
                    array_[i].write(out);
                }
                out += ']';
                break;
            }
            case Type::Object: {
                out += '{';
                for (std::size_t i = 0; i < object_.size(); ++i) {
                    if (i) out += ',';
                    write_string(out, object_[i].first);
                    out += ':';
                    object_[i].second.write(out);
                }
                out += '}';
                break;
            }
        }
    }

    static void write_string(std::string& out, const std::string& s) {
        out += '"';
        for (char c : s) {
            switch (c) {
                case '"': out += "\\\""; break;
                case '\\': out += "\\\\"; break;
                case '\n': out += "\\n"; break;
                case '\r': out += "\\r"; break;
                case '\t': out += "\\t"; break;
                default:
                    if (static_cast<unsigned char>(c) < 0x20) {
                        char buf[8];
                        std::snprintf(buf, sizeof(buf), "\\u%04x", c);
                        out += buf;
                    } else {
                        out.push_back(c);
                    }
            }
        }
        out += '"';
    }

    Type type_ = Type::Null;
    bool bool_ = false;
    double number_ = 0;
    std::string string_;
    std::vector<Value> array_;
    std::vector<std::pair<std::string, Value>> object_;
};

namespace detail {

class Parser {
public:
    explicit Parser(const std::string& text) : text_(text) {}

    Value parse() {
        skip_ws();
        Value value = parse_value();
        skip_ws();
        return value;
    }

private:
    const std::string& text_;
    std::size_t pos_ = 0;

    [[noreturn]] void fail(const char* what) const {
        throw std::runtime_error(std::string("malformed JSON: ") + what);
    }

    void skip_ws() {
        while (pos_ < text_.size()) {
            const char c = text_[pos_];
            if (c == ' ' || c == '\t' || c == '\n' || c == '\r') {
                ++pos_;
                continue;
            }
            break;
        }
    }

    char peek() {
        if (pos_ >= text_.size()) fail("unexpected end of input");
        return text_[pos_];
    }

    void expect(char c) {
        if (peek() != c) fail("unexpected character");
        ++pos_;
    }

    Value parse_value() {
        skip_ws();
        switch (peek()) {
            case '{': return parse_object();
            case '[': return parse_array();
            case '"': return Value(parse_string());
            case 't': case 'f': return parse_bool();
            case 'n': parse_literal("null"); return Value();
            default: return parse_number();
        }
    }

    void parse_literal(const char* literal) {
        const std::string expected(literal);
        if (text_.compare(pos_, expected.size(), expected) != 0) fail("bad literal");
        pos_ += expected.size();
    }

    Value parse_bool() {
        if (peek() == 't') {
            parse_literal("true");
            return Value(true);
        }
        parse_literal("false");
        return Value(false);
    }

    Value parse_number() {
        const std::size_t start = pos_;
        if (pos_ < text_.size() && (text_[pos_] == '-' || text_[pos_] == '+')) ++pos_;
        while (pos_ < text_.size()) {
            const char c = text_[pos_];
            if ((c >= '0' && c <= '9') || c == '.' || c == 'e' || c == 'E' || c == '+' || c == '-') {
                ++pos_;
                continue;
            }
            break;
        }
        if (pos_ == start) fail("invalid number");
        return Value(std::stod(text_.substr(start, pos_ - start)));
    }

    std::string parse_string() {
        expect('"');
        std::string out;
        while (true) {
            if (pos_ >= text_.size()) fail("unterminated string");
            const char c = text_[pos_++];
            if (c == '"') break;
            if (c != '\\') {
                out.push_back(c);
                continue;
            }
            if (pos_ >= text_.size()) fail("unterminated escape");
            const char esc = text_[pos_++];
            switch (esc) {
                case '"': out.push_back('"'); break;
                case '\\': out.push_back('\\'); break;
                case '/': out.push_back('/'); break;
                case 'n': out.push_back('\n'); break;
                case 't': out.push_back('\t'); break;
                case 'r': out.push_back('\r'); break;
                case 'b': out.push_back('\b'); break;
                case 'f': out.push_back('\f'); break;
                case 'u': {
                    if (pos_ + 4 > text_.size()) fail("bad unicode escape");
                    unsigned code = 0;
                    for (int k = 0; k < 4; ++k) {
                        const char h = text_[pos_++];
                        code <<= 4;
                        if (h >= '0' && h <= '9') code |= unsigned(h - '0');
                        else if (h >= 'a' && h <= 'f') code |= unsigned(h - 'a' + 10);
                        else if (h >= 'A' && h <= 'F') code |= unsigned(h - 'A' + 10);
                        else fail("bad unicode escape");
                    }
                    if (code < 0x80) {
                        out.push_back(char(code));
                    } else if (code < 0x800) {
                        out.push_back(char(0xC0 | (code >> 6)));
                        out.push_back(char(0x80 | (code & 0x3F)));
                    } else {
                        out.push_back(char(0xE0 | (code >> 12)));
                        out.push_back(char(0x80 | ((code >> 6) & 0x3F)));
                        out.push_back(char(0x80 | (code & 0x3F)));
                    }
                    break;
                }
                default: out.push_back(esc); break;
            }
        }
        return out;
    }

    Value parse_array() {
        expect('[');
        Value out = Value::array();
        skip_ws();
        if (peek() == ']') {
            ++pos_;
            return out;
        }
        while (true) {
            out.push(parse_value());
            skip_ws();
            const char c = peek();
            if (c == ',') {
                ++pos_;
                continue;
            }
            if (c == ']') {
                ++pos_;
                break;
            }
            fail("expected ',' or ']'");
        }
        return out;
    }

    Value parse_object() {
        expect('{');
        Value out = Value::object();
        skip_ws();
        if (peek() == '}') {
            ++pos_;
            return out;
        }
        while (true) {
            skip_ws();
            const std::string key = parse_string();
            skip_ws();
            expect(':');
            out.set(key, parse_value());
            skip_ws();
            const char c = peek();
            if (c == ',') {
                ++pos_;
                continue;
            }
            if (c == '}') {
                ++pos_;
                break;
            }
            fail("expected ',' or '}'");
        }
        return out;
    }
};

} // namespace detail

/// Parse a JSON document. Throws `std::runtime_error` on malformed input.
inline Value parse(const std::string& text) {
    if (text.empty()) return Value();
    return detail::Parser(text).parse();
}

} // namespace json
} // namespace ilink
