const locale = {
  'mongo.applyFilter': 'Apply',
  'mongo.collections': 'Collections',
  'mongo.databases': 'Databases',
  'mongo.docCount': '{count} docs',
  'mongo.documentDetail': 'Document',
  'mongo.documents': 'Documents',
  'mongo.filter': 'Filter',
  'mongo.insert': 'Insert',
  'mongo.noCollections': 'No collections in this database',
  'mongo.noDocuments': 'No matching documents',
  'mongo.noIdHint': 'Document must include an _id field to save or delete',
  'mongo.queries': 'Queries',
  'mongo.queryHint': 'Enter a MongoDB JSON command and press ⌘+Enter',
  'mongo.searchCollections': 'Search collections…',
  'mongo.selectCollection': 'Select a collection',
} as const;

export default locale;
export type MongoTranslationKey = keyof typeof locale;
