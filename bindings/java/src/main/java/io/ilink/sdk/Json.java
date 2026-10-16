package io.ilink.sdk;

/**
 * Tiny, dependency-free JSON helpers for the C ABI envelope.
 *
 * <p>The SDK always returns {@code {"ok":true,"result":...}} or
 * {@code {"ok":false,"error":"...","kind":"..."}}. We only need to inspect the
 * flat top-level fields, so a full JSON parser is unnecessary; results are
 * handed back to the caller as raw JSON for use with Gson/Jackson.
 */
final class Json {

    private Json() {}

    /** Return the raw JSON value for a top-level key, or {@code null}. */
    static String field(String object, String key) {
        int i = skipWhitespace(object, 0);
        if (i >= object.length() || object.charAt(i) != '{') {
            throw new IllegalStateException("SDK response is not a JSON object");
        }
        i++;
        while (true) {
            i = skipWhitespace(object, i);
            if (i < object.length() && object.charAt(i) == '}') {
                break;
            }
            if (i >= object.length() || object.charAt(i) != '"') {
                throw new IllegalStateException("malformed JSON object");
            }
            int keyEnd = skipString(object, i);
            String found = object.substring(i + 1, keyEnd - 1);
            i = skipWhitespace(object, keyEnd);
            if (i >= object.length() || object.charAt(i) != ':') {
                throw new IllegalStateException("malformed JSON object");
            }
            i = skipWhitespace(object, i + 1);
            int valueBegin = i;
            int valueEnd = skipValue(object, i);
            if (found.equals(key)) {
                return object.substring(valueBegin, valueEnd);
            }
            i = skipWhitespace(object, valueEnd);
            if (i < object.length() && object.charAt(i) == ',') {
                i++;
                continue;
            }
            break;
        }
        return null;
    }

    static String decodeString(String literal) {
        if (literal == null) {
            return null;
        }
        StringBuilder out = new StringBuilder(literal.length());
        for (int i = 1; i + 1 < literal.length(); i++) {
            char c = literal.charAt(i);
            if (c != '\\') {
                out.append(c);
                continue;
            }
            if (++i >= literal.length()) {
                break;
            }
            char escaped = literal.charAt(i);
            switch (escaped) {
                case '"': out.append('"'); break;
                case '\\': out.append('\\'); break;
                case '/': out.append('/'); break;
                case 'n': out.append('\n'); break;
                case 't': out.append('\t'); break;
                case 'r': out.append('\r'); break;
                case 'b': out.append('\b'); break;
                case 'f': out.append('\f'); break;
                case 'u':
                    if (i + 4 >= literal.length()) {
                        break;
                    }
                    out.append((char) Integer.parseInt(literal.substring(i + 1, i + 5), 16));
                    i += 4;
                    break;
                default: out.append(escaped);
            }
        }
        return out.toString();
    }

    static String quote(String value) {
        StringBuilder out = new StringBuilder(value.length() + 2).append('"');
        for (int i = 0; i < value.length(); i++) {
            char c = value.charAt(i);
            switch (c) {
                case '"': out.append("\\\""); break;
                case '\\': out.append("\\\\"); break;
                case '\n': out.append("\\n"); break;
                case '\r': out.append("\\r"); break;
                case '\t': out.append("\\t"); break;
                default:
                    if (c < 0x20) {
                        out.append(String.format("\\u%04x", (int) c));
                    } else {
                        out.append(c);
                    }
            }
        }
        return out.append('"').toString();
    }

    private static int skipWhitespace(String s, int i) {
        while (i < s.length()) {
            char c = s.charAt(i);
            if (c == ' ' || c == '\t' || c == '\n' || c == '\r') {
                i++;
                continue;
            }
            break;
        }
        return i;
    }

    private static int skipString(String s, int i) {
        i++;
        while (i < s.length()) {
            char c = s.charAt(i);
            if (c == '\\') {
                i += 2;
                continue;
            }
            if (c == '"') {
                return i + 1;
            }
            i++;
        }
        throw new IllegalStateException("malformed JSON string");
    }

    private static int skipValue(String s, int i) {
        i = skipWhitespace(s, i);
        char c = s.charAt(i);
        if (c == '"') {
            return skipString(s, i);
        }
        if (c == '{' || c == '[') {
            int depth = 0;
            while (i < s.length()) {
                char d = s.charAt(i);
                if (d == '"') {
                    i = skipString(s, i);
                    continue;
                }
                if (d == '{' || d == '[') {
                    depth++;
                    i++;
                    continue;
                }
                if (d == '}' || d == ']') {
                    depth--;
                    i++;
                    if (depth == 0) {
                        return i;
                    }
                    continue;
                }
                i++;
            }
            throw new IllegalStateException("unbalanced JSON value");
        }
        while (i < s.length() && s.charAt(i) != ',' && s.charAt(i) != '}' && s.charAt(i) != ']') {
            i++;
        }
        while (i > 0 && Character.isWhitespace(s.charAt(i - 1))) {
            i--;
        }
        return i;
    }
}
