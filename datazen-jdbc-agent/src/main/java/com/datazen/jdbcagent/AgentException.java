package com.datazen.jdbcagent;

final class AgentException extends Exception {
  final int code;
  final String category;

  AgentException(int code, String category, String message) {
    super(message);
    this.code = code;
    this.category = category;
  }

  static AgentException connect(String message) {
    return new AgentException(-32001, "connect", message);
  }

  static AgentException driver(String message) {
    return new AgentException(-32002, "driver", message);
  }

  static AgentException sql(String message) {
    return new AgentException(-32003, "sql", message);
  }

  static AgentException cancel(String message) {
    return new AgentException(-32004, "cancel", message);
  }

  static AgentException internal(String message) {
    return new AgentException(-32000, "internal", message);
  }
}
