import { legacyInvoke as invoke } from '@datazen/backend-client';

export type ArtifactExportResponse =
  | { status: 'cancelled' }
  | { status: 'saved'; artifactId: string; byteLength: number };

/** The Host owns identity, partial-result confirmation and the save destination. */
export const artifactCommands = {
  exportArtifact: (artifactId: string): Promise<ArtifactExportResponse> =>
    invoke<ArtifactExportResponse>('export_artifact', { request: { artifactId } }),
};
