package io.ilink.sdk

/**
 * Tiny, dependency-free JSON helpers for the C ABI envelope.
 *
 * The SDK always returns `{"ok":true,"result":...}` or
 * `{"ok":false,"error":"...","kind":"..."}`. Only the flat top-level fields are
 * needed, so a full JSON parser is unnecessary; results are handed back to the
 * caller as raw JSON for use with kotlinx.serialization / Gson / Jackson.
 */
internal object Json {

    /** Return the raw JSON value for a top-level key, or `null`. */
    fun field(obj: String, key: String): String? {
        var i = skipWhitespace(obj, 0)
        if (i >= obj.length || obj[i] != '{') {
            throw IllegalStateException("SDK response is not a JSON object")
        }
        i++
        while (true) {
            i = skipWhitespace(obj, i)
            if (i < obj.length && obj[i] == '}') break
            if (i >= obj.length || obj[i] != '"') {
                throw IllegalStateException("malformed JSON object")
            }
            val keyEnd = skipString(obj, i)
            val found = obj.substring(i + 1, keyEnd - 1)
            i = skipWhitespace(obj, keyEnd)
            if (i >= obj.length || obj[i] != ':') {
                throw IllegalStateException("malformed JSON object")
            }
            i = skipWhitespace(obj, i + 1)
            val valueBegin = i
            val valueEnd = skipValue(obj, i)
            if (found == key) return obj.substring(valueBegin, valueEnd)
            i = skipWhitespace(obj, valueEnd)
            if (i < obj.length && obj[i] == ',') {
                i++
                continue
            }
            break
        }
        return null
    }

    fun decodeString(literal: String?): String? {
        if (literal == null) return null
        val out = StringBuilder(literal.length)
        var i = 1
        while (i + 1 < literal.length) {
            val c = literal[i]
            if (c != '\\') {
                out.append(c)
                i++
                continue
            }
            i++
            if (i >= literal.length) break
            when (val escaped = literal[i]) {
                '"' -> out.append('"')
                '\\' -> out.append('\\')
                '/' -> out.append('/')
                'n' -> out.append('\n')
                't' -> out.append('\t')
                'r' -> out.append('\r')
                'b' -> out.append('\b')
                'f' -> out.append('\u000C')
                'u' -> {
                    if (i + 4 < literal.length) {
                        out.append(literal.substring(i + 1, i + 5).toInt(16).toChar())
                        i += 4
                    }
                }
                else -> out.append(escaped)
            }
            i++
        }
        return out.toString()
    }

    fun quote(value: String): String {
        val out = StringBuilder(value.length + 2).append('"')
        for (c in value) {
            when (c) {
                '"' -> out.append("\\\"")
                '\\' -> out.append("\\\\")
                '\n' -> out.append("\\n")
                '\r' -> out.append("\\r")
                '\t' -> out.append("\\t")
                else -> if (c < ' ') out.append("\\u%04x".format(c.code)) else out.append(c)
            }
        }
        return out.append('"').toString()
    }

    private fun skipWhitespace(s: String, start: Int): Int {
        var i = start
        while (i < s.length && (s[i] == ' ' || s[i] == '\t' || s[i] == '\n' || s[i] == '\r')) i++
        return i
    }

    private fun skipString(s: String, start: Int): Int {
        var i = start + 1
        while (i < s.length) {
            when (s[i]) {
                '\\' -> i += 2
                '"' -> return i + 1
                else -> i++
            }
        }
        throw IllegalStateException("malformed JSON string")
    }

    private fun skipValue(s: String, start: Int): Int {
        var i = skipWhitespace(s, start)
        when (s[i]) {
            '"' -> return skipString(s, i)
            '{', '[' -> {
                var depth = 0
                while (i < s.length) {
                    when (s[i]) {
                        '"' -> i = skipString(s, i)
                        '{', '[' -> { depth++; i++ }
                        '}', ']' -> {
                            depth--
                            i++
                            if (depth == 0) return i
                        }
                        else -> i++
                    }
                }
                throw IllegalStateException("unbalanced JSON value")
            }
            else -> {
                while (i < s.length && s[i] != ',' && s[i] != '}' && s[i] != ']') i++
                while (i > 0 && s[i - 1].isWhitespace()) i--
                return i
            }
        }
    }
}
