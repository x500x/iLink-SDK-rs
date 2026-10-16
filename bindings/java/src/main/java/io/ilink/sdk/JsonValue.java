package io.ilink.sdk;

import java.util.ArrayList;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * A minimal, dependency-free JSON value used by the typed models.
 *
 * <p>The SDK speaks JSON; rather than force every caller to add Gson/Jackson,
 * this class parses an operation result and exposes typed accessors. It is
 * intentionally small and only supports what the SDK emits.
 */
public final class JsonValue {

    public enum Type { NULL, BOOL, NUMBER, STRING, ARRAY, OBJECT }

    private static final JsonValue NULL = new JsonValue(Type.NULL, null);

    private final Type type;
    private final Object value;

    private JsonValue(Type type, Object value) {
        this.type = type;
        this.value = value;
    }

    public static JsonValue ofNull() {
        return NULL;
    }

    public static JsonValue of(boolean value) {
        return new JsonValue(Type.BOOL, value);
    }

    public static JsonValue of(double value) {
        return new JsonValue(Type.NUMBER, value);
    }

    public static JsonValue of(long value) {
        return new JsonValue(Type.NUMBER, (double) value);
    }

    public static JsonValue of(String value) {
        return value == null ? NULL : new JsonValue(Type.STRING, value);
    }

    public static JsonValue object() {
        return new JsonValue(Type.OBJECT, new LinkedHashMap<String, JsonValue>());
    }

    public static JsonValue array() {
        return new JsonValue(Type.ARRAY, new ArrayList<JsonValue>());
    }

    public Type type() {
        return type;
    }

    public boolean isNull() {
        return type == Type.NULL;
    }

    public boolean isObject() {
        return type == Type.OBJECT;
    }

    public boolean isArray() {
        return type == Type.ARRAY;
    }

    // -- accessors ----------------------------------------------------------

    public boolean asBoolean(boolean fallback) {
        if (type == Type.BOOL) {
            return (Boolean) value;
        }
        if (type == Type.NUMBER) {
            return ((Double) value) != 0;
        }
        return fallback;
    }

    public long asLong(long fallback) {
        return type == Type.NUMBER ? (long) (double) (Double) value : fallback;
    }

    public String asString(String fallback) {
        return type == Type.STRING ? (String) value : fallback;
    }

    /** The value for {@code key}, or a shared null value when absent. */
    @SuppressWarnings("unchecked")
    public JsonValue get(String key) {
        if (type != Type.OBJECT) {
            return NULL;
        }
        JsonValue found = ((Map<String, JsonValue>) value).get(key);
        return found == null ? NULL : found;
    }

    /** The element at {@code index}, or a shared null value when out of range. */
    @SuppressWarnings("unchecked")
    public JsonValue at(int index) {
        if (type != Type.ARRAY) {
            return NULL;
        }
        List<JsonValue> list = (List<JsonValue>) value;
        return index >= 0 && index < list.size() ? list.get(index) : NULL;
    }

    @SuppressWarnings("unchecked")
    public int size() {
        if (type == Type.ARRAY) {
            return ((List<JsonValue>) value).size();
        }
        if (type == Type.OBJECT) {
            return ((Map<String, JsonValue>) value).size();
        }
        return 0;
    }

    @SuppressWarnings("unchecked")
    public List<JsonValue> items() {
        if (type != Type.ARRAY) {
            return Collections.emptyList();
        }
        return (List<JsonValue>) value;
    }

    @SuppressWarnings("unchecked")
    public void put(String key, JsonValue item) {
        ((Map<String, JsonValue>) value).put(key, item == null ? NULL : item);
    }

    @SuppressWarnings("unchecked")
    public void add(JsonValue item) {
        ((List<JsonValue>) value).add(item == null ? NULL : item);
    }

    // -- serialisation ------------------------------------------------------

    public String dump() {
        StringBuilder out = new StringBuilder();
        write(out);
        return out.toString();
    }

    private void write(StringBuilder out) {
        switch (type) {
            case NULL:
                out.append("null");
                break;
            case BOOL:
                out.append(((Boolean) value) ? "true" : "false");
                break;
            case NUMBER: {
                double number = (Double) value;
                long rounded = (long) number;
                out.append(number == (double) rounded ? Long.toString(rounded) : Double.toString(number));
                break;
            }
            case STRING:
                writeString(out, (String) value);
                break;
            case ARRAY:
                out.append('[');
                for (int i = 0; i < items().size(); i++) {
                    if (i > 0) {
                        out.append(',');
                    }
                    items().get(i).write(out);
                }
                out.append(']');
                break;
            case OBJECT:
                out.append('{');
                boolean first = true;
                for (Map.Entry<String, JsonValue> entry : entries().entrySet()) {
                    if (!first) {
                        out.append(',');
                    }
                    first = false;
                    writeString(out, entry.getKey());
                    out.append(':');
                    entry.getValue().write(out);
                }
                out.append('}');
                break;
        }
    }

    @SuppressWarnings("unchecked")
    public Map<String, JsonValue> entries() {
        if (type != Type.OBJECT) {
            return Collections.emptyMap();
        }
        return (Map<String, JsonValue>) value;
    }

    static void writeString(StringBuilder out, String s) {
        out.append('"');
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
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
        out.append('"');
    }

    // -- parsing ------------------------------------------------------------

    /** Parse a JSON document. Throws {@link IllegalArgumentException} on bad input. */
    public static JsonValue parse(String text) {
        if (text == null || text.isEmpty()) {
            return NULL;
        }
        Parser parser = new Parser(text);
        JsonValue value = parser.parseValue();
        parser.skipWhitespace();
        return value;
    }

    private static final class Parser {
        private final String text;
        private int pos;

        Parser(String text) {
            this.text = text;
        }

        JsonValue parseValue() {
            skipWhitespace();
            if (pos >= text.length()) {
                throw new IllegalArgumentException("unexpected end of JSON input");
            }
            char c = text.charAt(pos);
            switch (c) {
                case '{': return parseObject();
                case '[': return parseArray();
                case '"': return JsonValue.of(parseString());
                case 't': expectLiteral("true"); return JsonValue.of(true);
                case 'f': expectLiteral("false"); return JsonValue.of(false);
                case 'n': expectLiteral("null"); return JsonValue.ofNull();
                default: return parseNumber();
            }
        }

        void skipWhitespace() {
            while (pos < text.length()) {
                char c = text.charAt(pos);
                if (c == ' ' || c == '\t' || c == '\n' || c == '\r') {
                    pos++;
                    continue;
                }
                break;
            }
        }

        private void expectLiteral(String literal) {
            if (!text.startsWith(literal, pos)) {
                throw new IllegalArgumentException("bad JSON literal at " + pos);
            }
            pos += literal.length();
        }

        private JsonValue parseNumber() {
            int start = pos;
            while (pos < text.length()) {
                char c = text.charAt(pos);
                if ((c >= '0' && c <= '9') || c == '-' || c == '+' || c == '.' || c == 'e' || c == 'E') {
                    pos++;
                    continue;
                }
                break;
            }
            if (pos == start) {
                throw new IllegalArgumentException("invalid JSON number at " + start);
            }
            return JsonValue.of(Double.parseDouble(text.substring(start, pos)));
        }

        private String parseString() {
            expect('"');
            StringBuilder out = new StringBuilder();
            while (pos < text.length()) {
                char c = text.charAt(pos++);
                if (c == '"') {
                    return out.toString();
                }
                if (c != '\\') {
                    out.append(c);
                    continue;
                }
                if (pos >= text.length()) {
                    throw new IllegalArgumentException("unterminated escape");
                }
                char esc = text.charAt(pos++);
                switch (esc) {
                    case '"': out.append('"'); break;
                    case '\\': out.append('\\'); break;
                    case '/': out.append('/'); break;
                    case 'n': out.append('\n'); break;
                    case 't': out.append('\t'); break;
                    case 'r': out.append('\r'); break;
                    case 'b': out.append('\b'); break;
                    case 'f': out.append('\f'); break;
                    case 'u':
                        if (pos + 4 > text.length()) {
                            throw new IllegalArgumentException("bad unicode escape");
                        }
                        out.append((char) Integer.parseInt(text.substring(pos, pos + 4), 16));
                        pos += 4;
                        break;
                    default: out.append(esc);
                }
            }
            throw new IllegalArgumentException("unterminated JSON string");
        }

        private void expect(char c) {
            if (pos >= text.length() || text.charAt(pos) != c) {
                throw new IllegalArgumentException("expected '" + c + "' at " + pos);
            }
            pos++;
        }

        private JsonValue parseArray() {
            expect('[');
            JsonValue out = JsonValue.array();
            skipWhitespace();
            if (pos < text.length() && text.charAt(pos) == ']') {
                pos++;
                return out;
            }
            while (true) {
                out.add(parseValue());
                skipWhitespace();
                char c = text.charAt(pos);
                if (c == ',') {
                    pos++;
                    continue;
                }
                if (c == ']') {
                    pos++;
                    return out;
                }
                throw new IllegalArgumentException("expected ',' or ']' at " + pos);
            }
        }

        private JsonValue parseObject() {
            expect('{');
            JsonValue out = JsonValue.object();
            skipWhitespace();
            if (pos < text.length() && text.charAt(pos) == '}') {
                pos++;
                return out;
            }
            while (true) {
                skipWhitespace();
                String key = parseString();
                skipWhitespace();
                expect(':');
                out.put(key, parseValue());
                skipWhitespace();
                char c = text.charAt(pos);
                if (c == ',') {
                    pos++;
                    continue;
                }
                if (c == '}') {
                    pos++;
                    return out;
                }
                throw new IllegalArgumentException("expected ',' or '}' at " + pos);
            }
        }
    }
}
