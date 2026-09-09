package com.datazen.jdbcagent;

import java.math.BigDecimal;
import java.sql.ResultSet;
import java.sql.ResultSetMetaData;
import java.sql.SQLException;
import java.sql.Types;
import java.util.ArrayList;
import java.util.Base64;
import java.util.List;

final class TypeCodec {

  private TypeCodec() {}

  static String columnsJson(ResultSetMetaData md) throws SQLException {
    StringBuilder sb = new StringBuilder("[");
    int n = md.getColumnCount();
    for (int i = 1; i <= n; i++) {
      if (i > 1) {
        sb.append(',');
      }
      String name = md.getColumnLabel(i);
      if (name == null || name.isEmpty()) {
        name = md.getColumnName(i);
      }
      String type = md.getColumnTypeName(i);
      if (type == null) {
        type = "UNKNOWN";
      }
      sb.append("{\"name\":")
          .append(JsonLite.quote(name))
          .append(",\"type\":")
          .append(JsonLite.quote(type))
          .append('}');
    }
    sb.append(']');
    return sb.toString();
  }

  static List<String> readRows(ResultSet rs, int maxRows) throws SQLException {
    ResultSetMetaData md = rs.getMetaData();
    int cols = md.getColumnCount();
    List<String> rows = new ArrayList<>();
    int count = 0;
    while (count < maxRows && rs.next()) {
      StringBuilder row = new StringBuilder("[");
      for (int i = 1; i <= cols; i++) {
        if (i > 1) {
          row.append(',');
        }
        row.append(cellJson(rs, i, md.getColumnType(i)));
      }
      row.append(']');
      rows.add(row.toString());
      count++;
    }
    return rows;
  }

  static String rowsJson(List<String> rowJsons) {
    StringBuilder sb = new StringBuilder("[");
    for (int i = 0; i < rowJsons.size(); i++) {
      if (i > 0) {
        sb.append(',');
      }
      sb.append(rowJsons.get(i));
    }
    sb.append(']');
    return sb.toString();
  }

  private static String cellJson(ResultSet rs, int i, int sqlType) throws SQLException {
    Object v = rs.getObject(i);
    if (v == null || rs.wasNull()) {
      return "null";
    }
    switch (sqlType) {
      case Types.BOOLEAN:
      case Types.BIT:
        return rs.getBoolean(i) ? "true" : "false";
      case Types.TINYINT:
      case Types.SMALLINT:
      case Types.INTEGER:
        return Integer.toString(rs.getInt(i));
      case Types.BIGINT:
        return Long.toString(rs.getLong(i));
      case Types.REAL:
      case Types.FLOAT:
      case Types.DOUBLE:
        return Double.toString(rs.getDouble(i));
      case Types.DECIMAL:
      case Types.NUMERIC:
        BigDecimal bd = rs.getBigDecimal(i);
        return bd == null ? "null" : JsonLite.quote(bd.toPlainString());
      case Types.BINARY:
      case Types.VARBINARY:
      case Types.LONGVARBINARY:
      case Types.BLOB:
        byte[] bytes = rs.getBytes(i);
        if (bytes == null) {
          return "null";
        }
        return JsonLite.quote(Base64.getEncoder().encodeToString(bytes));
      case Types.DATE:
      case Types.TIME:
      case Types.TIMESTAMP:
      case Types.TIMESTAMP_WITH_TIMEZONE:
      case Types.TIME_WITH_TIMEZONE:
        return JsonLite.quote(String.valueOf(v));
      default:
        return JsonLite.quote(String.valueOf(v));
    }
  }
}
