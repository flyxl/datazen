package com.datazen.jdbcagent;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/** Tiny helpers for extracting fields from JSON-RPC request lines. */
final class JsonLite {

  private JsonLite() {}

  static String escape(String s) {
    if (s == null) {
      return "";
    }
    return s.replace("\\", "\\\\")
        .replace("\"", "\\\"")
        .replace("\n", "\\n")
        .replace("\r", "\\r")
        .replace("\t", "\\t");
  }

  static String quote(String s) {
    return "\"" + escape(s) + "\"";
  }

  static String extractRaw(String json, String key) {
    String pattern = "\"" + key + "\"";
    int i = json.indexOf(pattern);
    if (i < 0) {
      return null;
    }
    int colon = json.indexOf(':', i + pattern.length());
    if (colon < 0) {
      return null;
    }
    int j = colon + 1;
    while (j < json.length() && Character.isWhitespace(json.charAt(j))) {
      j++;
    }
    if (j >= json.length()) {
      return null;
    }
    if (json.charAt(j) == '"') {
      int end = findStringEnd(json, j);
      if (end < 0) {
        return null;
      }
      return json.substring(j, end + 1);
    }
    if (json.charAt(j) == '{' || json.charAt(j) == '[') {
      int end = skipBalanced(json, j);
      return json.substring(j, end);
    }
    int end = j;
    while (end < json.length()) {
      char c = json.charAt(end);
      if (c == ',' || c == '}' || Character.isWhitespace(c)) {
        break;
      }
      end++;
    }
    return json.substring(j, end);
  }

  static String extractString(String json, String key) {
    String raw = extractRaw(json, key);
    if (raw == null) {
      return null;
    }
    if (raw.startsWith("\"") && raw.endsWith("\"") && raw.length() >= 2) {
      return unescape(raw.substring(1, raw.length() - 1));
    }
    if ("null".equals(raw)) {
      return null;
    }
    return raw;
  }

  static long extractLong(String json, String key, long defaultValue) {
    String raw = extractRaw(json, key);
    if (raw == null || raw.isEmpty() || "null".equals(raw)) {
      return defaultValue;
    }
    try {
      return Long.parseLong(raw.replace("\"", ""));
    } catch (NumberFormatException e) {
      return defaultValue;
    }
  }

  static int extractInt(String json, String key, int defaultValue) {
    return (int) extractLong(json, key, defaultValue);
  }

  /** Extract string array values for key (simple, non-nested strings only). */
  static List<String> extractStringArray(String json, String key) {
    List<String> out = new ArrayList<>();
    String raw = extractRaw(json, key);
    if (raw == null || !raw.startsWith("[")) {
      return out;
    }
    int i = 1;
    while (i < raw.length()) {
      while (i < raw.length() && (Character.isWhitespace(raw.charAt(i)) || raw.charAt(i) == ',')) {
        i++;
      }
      if (i >= raw.length() || raw.charAt(i) == ']') {
        break;
      }
      if (raw.charAt(i) == '"') {
        int end = findStringEnd(raw, i);
        if (end < 0) {
          break;
        }
        out.add(unescape(raw.substring(i + 1, end)));
        i = end + 1;
      } else {
        break;
      }
    }
    return out;
  }

  /** Flat string props object: {"a":"b","c":"d"}. */
  static Map<String, String> extractStringMap(String json, String key) {
    Map<String, String> map = new LinkedHashMap<>();
    String raw = extractRaw(json, key);
    if (raw == null || !raw.startsWith("{")) {
      return map;
    }
    int i = 1;
    while (i < raw.length()) {
      while (i < raw.length() && (Character.isWhitespace(raw.charAt(i)) || raw.charAt(i) == ',')) {
        i++;
      }
      if (i >= raw.length() || raw.charAt(i) == '}') {
        break;
      }
      if (raw.charAt(i) != '"') {
        break;
      }
      int kEnd = findStringEnd(raw, i);
      if (kEnd < 0) {
        break;
      }
      String k = unescape(raw.substring(i + 1, kEnd));
      i = kEnd + 1;
      while (i < raw.length() && Character.isWhitespace(raw.charAt(i))) {
        i++;
      }
      if (i >= raw.length() || raw.charAt(i) != ':') {
        break;
      }
      i++;
      while (i < raw.length() && Character.isWhitespace(raw.charAt(i))) {
        i++;
      }
      if (i >= raw.length() || raw.charAt(i) != '"') {
        break;
      }
      int vEnd = findStringEnd(raw, i);
      if (vEnd < 0) {
        break;
      }
      String v = unescape(raw.substring(i + 1, vEnd));
      map.put(k, v);
      i = vEnd + 1;
    }
    return map;
  }

  private static int findStringEnd(String s, int openQuote) {
    int i = openQuote + 1;
    while (i < s.length()) {
      char c = s.charAt(i);
      if (c == '\\') {
        i += 2;
        continue;
      }
      if (c == '"') {
        return i;
      }
      i++;
    }
    return -1;
  }

  private static int skipBalanced(String s, int start) {
    char open = s.charAt(start);
    char close = open == '{' ? '}' : ']';
    int depth = 0;
    boolean inStr = false;
    for (int i = start; i < s.length(); i++) {
      char c = s.charAt(i);
      if (inStr) {
        if (c == '\\') {
          i++;
          continue;
        }
        if (c == '"') {
          inStr = false;
        }
        continue;
      }
      if (c == '"') {
        inStr = true;
        continue;
      }
      if (c == open) {
        depth++;
      } else if (c == close) {
        depth--;
        if (depth == 0) {
          return i + 1;
        }
      }
    }
    return s.length();
  }

  private static String unescape(String s) {
    StringBuilder sb = new StringBuilder(s.length());
    for (int i = 0; i < s.length(); i++) {
      char c = s.charAt(i);
      if (c == '\\' && i + 1 < s.length()) {
        char n = s.charAt(++i);
        switch (n) {
          case 'n' -> sb.append('\n');
          case 'r' -> sb.append('\r');
          case 't' -> sb.append('\t');
          case '"' -> sb.append('"');
          case '\\' -> sb.append('\\');
          case '/' -> sb.append('/');
          default -> sb.append(n);
        }
      } else {
        sb.append(c);
      }
    }
    return sb.toString();
  }
}
