const locale = {
  'mongo.applyFilter': '應用',
  'mongo.collections': '集合',
  'mongo.databases': '數據庫',
  'mongo.docCount': '{count} 條',
  'mongo.documentDetail': '文檔詳情',
  'mongo.documents': '文檔瀏覽',
  'mongo.filter': 'Filter',
  'mongo.insert': '插入',
  'mongo.noCollections': '該庫沒有集合',
  'mongo.noDocuments': '沒有匹配的文檔',
  'mongo.noIdHint': '文檔需包含 _id 字段才能保存或刪除',
  'mongo.queries': '命令',
  'mongo.queryHint': '輸入 MongoDB JSON 命令並按 ⌘+Enter 執行',
  'mongo.searchCollections': '搜索集合…',
  'mongo.selectCollection': '選擇一個集合',
} as const;

export default locale;
export type MongoTranslationKey = keyof typeof locale;
