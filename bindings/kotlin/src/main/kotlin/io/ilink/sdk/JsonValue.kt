package io.ilink.sdk

/**
 * A minimal, dependency-free JSON value used by the typed models.
 *
 * The SDK speaks JSON; rather than require kotlinx.serialization / Gson for the
 * common flows, this class parses an operation result and exposes typed
 * accessors. It is intentionally small and only supports what the SDK emits.
 */
class JsonValue private constructor(
    val type: Type,
    private val raw: Any?,
) {

    enum class Type { NULL, BOOL, NUMBER, STRING, ARRAY, OBJECT }

    val isNull: Boolean get() = type == Type.NULL
    val isObject: Boolean get() = type == Type.OBJECT
    val isArray: Boolean get() = type == Type.ARRAY

    // -- accessors ----------------------------------------------------------

    fun asBoolean(fallback: Boolean = false): Boolean = when (type) {
        Type.BOOL -> raw as Boolean
        Type.NUMBER -> (raw as Double) != 0.0
        else -> fallback
    }

    fun asLong(fallback: Long = 0): Long = if (type == Type.NUMBER) (raw as Double).toLong() else fallback

    fun asString(fallback: String = ""): String = if (type == Type.STRING) raw as String else fallback

    /** The value for [key], or a shared null value when absent. */
    @Suppress("UNCHECKED_CAST")
    operator fun get(key: String): JsonValue {
        if (type != Type.OBJECT) return NULL
        return (raw as Map<String, JsonValue>)[key] ?: NULL
    }

    /** The element at [index], or a shared null value when out of range. */
    @Suppress("UNCHECKED_CAST")
    fun at(index: Int): JsonValue {
        if (type != Type.ARRAY) return NULL
        val list = raw as List<JsonValue>
        return if (index in list.indices) list[index] else NULL
    }

    @Suppress("UNCHECKED_CAST")
    val size: Int
        get() = when (type) {
            Type.ARRAY -> (raw as List<JsonValue>).size
            Type.OBJECT -> (raw as Map<String, JsonValue>).size
            else -> 0
        }

    @Suppress("UNCHECKED_CAST")
    val items: List<JsonValue>
        get() = if (type == Type.ARRAY) raw as List<JsonValue> else emptyList()

    @Suppress("UNCHECKED_CAST")
    val entries: Map<String, JsonValue>
        get() = if (type == Type.OBJECT) raw as Map<String, JsonValue> else emptyMap()

    @Suppress("UNCHECKED_CAST")
    fun put(key: String, value: JsonValue) {
        (raw as MutableMap<String, JsonValue>)[key] = value
    }

    @Suppress("UNCHECKED_CAST")
    fun add(value: JsonValue) {
        (raw as MutableList<JsonValue>).add(value)
    }

    // -- serialisation ------------------------------------------------------

    fun dump(): String = buildString { write(this) }

    private fun write(out: StringBuilder) {
        when (type) {
            Type.NULL -> out.append("null")
            Type.BOOL -> out.append(if (raw as Boolean) "true" else "false")
            Type.NUMBER -> {
                val number = raw as Double
                val rounded = number.toLong()
                out.append(if (number == rounded.toDouble()) rounded.toString() else number.toString())
            }
            Type.STRING -> writeString(out, raw as String)
            Type.ARRAY -> {
                out.append('[')
                items.forEachIndexed { index, item ->
                    if (index > 0) out.append(',')
                    item.write(out)
                }
                out.append(']')
            }
            Type.OBJECT -> {
                out.append('{')
                var first = true
                for ((key, value) in entries) {
                    if (!first) out.append(',')
                    first = false
                    writeString(out, key)
                    out.append(':')
                    value.write(out)
                }
                out.append('}')
            }
        }
    }

    companion object {
        val NULL = JsonValue(Type.NULL, null)

        fun of(value: Boolean) = JsonValue(Type.BOOL, value)
        fun of(value: Double) = JsonValue(Type.NUMBER, value)
        fun of(value: Long) = JsonValue(Type.NUMBER, value.toDouble())
        fun of(value: Int) = JsonValue(Type.NUMBER, value.toDouble())
        fun of(value: String?) = if (value == null) NULL else JsonValue(Type.STRING, value)

        fun objectValue() = JsonValue(Type.OBJECT, LinkedHashMap<String, JsonValue>())
        fun arrayValue() = JsonValue(Type.ARRAY, ArrayList<JsonValue>())

        private fun writeString(out: StringBuilder, s: String) {
            out.append('"')
            for (c in s) {
                when (c) {
                    '"' -> out.append("\\\"")
                    '\\' -> out.append("\\\\")
                    '\n' -> out.append("\\n")
                    '\r' -> out.append("\\r")
                    '\t' -> out.append("\\t")
                    else -> if (c < ' ') out.append("\\u%04x".format(c.code)) else out.append(c)
                }
            }
            out.append('"')
        }

        /** Parse a JSON document. Throws [IllegalArgumentException] on bad input. */
        fun parse(text: String?): JsonValue {
            if (text.isNullOrEmpty()) return NULL
            val parser = Parser(text)
            return parser.parseValue()
        }
    }

    private class Parser(private val text: String) {
        private var pos = 0

        fun parseValue(): JsonValue {
            skipWhitespace()
            require(pos < text.length) { "unexpected end of JSON input" }
            return when (text[pos]) {
                '{' -> parseObject()
                '[' -> parseArray()
                '"' -> of(parseString())
                't' -> { expectLiteral("true"); of(true) }
                'f' -> { expectLiteral("false"); of(false) }
                'n' -> { expectLiteral("null"); NULL }
                else -> parseNumber()
            }
        }

        private fun skipWhitespace() {
            while (pos < text.length && text[pos].isWhitespace()) pos++
        }

        private fun expectLiteral(literal: String) {
            require(text.startsWith(literal, pos)) { "bad JSON literal at $pos" }
            pos += literal.length
        }

        private fun parseNumber(): JsonValue {
            val start = pos
            while (pos < text.length) {
                val c = text[pos]
                if (c.isDigit() || c == '-' || c == '+' || c == '.' || c == 'e' || c == 'E') pos++ else break
            }
            require(pos > start) { "invalid JSON number at $start" }
            return of(text.substring(start, pos).toDouble())
        }

        private fun parseString(): String {
            expect('"')
            val out = StringBuilder()
            while (pos < text.length) {
                val c = text[pos++]
                if (c == '"') return out.toString()
                if (c != '\\') {
                    out.append(c)
                    continue
                }
                require(pos < text.length) { "unterminated escape" }
                when (val escaped = text[pos++]) {
                    '"' -> out.append('"')
                    '\\' -> out.append('\\')
                    '/' -> out.append('/')
                    'n' -> out.append('\n')
                    't' -> out.append('\t')
                    'r' -> out.append('\r')
                    'b' -> out.append('\b')
                    'f' -> out.append('\u000C')
                    'u' -> {
                        require(pos + 4 <= text.length) { "bad unicode escape" }
                        out.append(text.substring(pos, pos + 4).toInt(16).toChar())
                        pos += 4
                    }
                    else -> out.append(escaped)
                }
            }
            throw IllegalArgumentException("unterminated JSON string")
        }

        private fun expect(c: Char) {
            require(pos < text.length && text[pos] == c) { "expected '$c' at $pos" }
            pos++
        }

        private fun parseArray(): JsonValue {
            expect('[')
            val out = arrayValue()
            skipWhitespace()
            if (pos < text.length && text[pos] == ']') {
                pos++
                return out
            }
            while (true) {
                out.add(parseValue())
                skipWhitespace()
                when (text[pos]) {
                    ',' -> pos++
                    ']' -> { pos++; return out }
                    else -> throw IllegalArgumentException("expected ',' or ']' at $pos")
                }
            }
        }

        private fun parseObject(): JsonValue {
            expect('{')
            val out = objectValue()
            skipWhitespace()
            if (pos < text.length && text[pos] == '}') {
                pos++
                return out
            }
            while (true) {
                skipWhitespace()
                val key = parseString()
                skipWhitespace()
                expect(':')
                out.put(key, parseValue())
                skipWhitespace()
                when (text[pos]) {
                    ',' -> pos++
                    '}' -> { pos++; return out }
                    else -> throw IllegalArgumentException("expected ',' or '}' at $pos")
                }
            }
        }
    }
}
