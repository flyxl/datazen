/**
 * Re-export of the shared relation-metadata cache.
 *
 * The cache is a process-wide singleton shared by the editor and the Visual
 * Query Builder, so it lives in `src/lib/relationMetadata` rather than inside the
 * editor. Importing it from here still yields the same instance.
 */

export {
  createMetadataCache,
  editorRelationKey,
  metadataCache,
  relationIdentityKey,
  schemaToMetadata,
} from '../../../lib/relationMetadata/metadataCache';
export type { MetadataCache, MetadataCacheDeps } from '../../../lib/relationMetadata/metadataCache';
