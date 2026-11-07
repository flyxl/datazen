const locale = {
  'mongo.applyFilter': '应用',
  'mongo.collections': '集合',
  'mongo.databases': '数据库',
  'mongo.docCount': '{count} 条',
  'mongo.documentDetail': '文档详情',
  'mongo.documents': '文档浏览',
  'mongo.filter': 'Filter',
  'mongo.insert': '插入',
  'mongo.noCollections': '该库没有集合',
  'mongo.noDocuments': '没有匹配的文档',
  'mongo.noIdHint': '文档需包含 _id 字段才能保存或删除',
  'mongo.queries': '命令',
  'mongo.queryHint': '输入 MongoDB JSON 命令并按 ⌘+Enter 执行',
  'mongo.searchCollections': '搜索集合…',
  'mongo.selectCollection': '选择一个集合',
} as const;

export default locale;
export type MongoTranslationKey = keyof typeof locale;
