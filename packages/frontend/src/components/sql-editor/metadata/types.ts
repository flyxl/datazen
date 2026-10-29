/**
 * Re-export of the shared relation-metadata contracts.
 *
 * The contracts moved to `src/lib/relationMetadata` when the Visual Query Builder
 * started reading the same cache: a shared service must not live inside one of
 * its consumers. This path is kept so editor code keeps importing from here.
 */

export type {
  EditorMetadataContext,
  EditorMetadataSnapshot,
  EditorRelationKey,
  EditorRelationKind,
  EditorRelationMetadata,
  EditorRelationRequest,
  EditorRelationSourceKind,
} from '../../../lib/relationMetadata/types';
