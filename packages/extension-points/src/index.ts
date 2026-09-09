/**
 * @datazen/extension-points — host privileged extension point contracts.
 */

export type { ExtensionPoint, CreateExtensionPointOptions } from './extensionPoints';
export { createExtensionPoint, ExtensionRegistry, extensionRegistry } from './extensionPoints';
export { useExtension, useIsExtensionEnhanced } from './useExtension';
export type {
  Disposable,
  ExtensionSubscription,
  ExtensionContext,
  ExtensionModule,
} from './lifecycle';
export { HostExtensionLoader, hostExtensionLoader, registerExtensionPoint } from './lifecycle';
export { SafeCompartmentWrapper, type SafeCompartmentOptions } from './safeCompartment';
export type {
  SqlEditorEnhancedFeatures,
  SqlEditorEnhancedOptions,
  SqlEditorProFeatures,
  SqlEditorProOptions,
  ExtensionSettingOption,
  ExtensionSettingItem,
  ExtensionSettingRenderProps,
  ExtensionGroupRenderProps,
  ExtensionSettingsContribution,
} from './sqlEditorEnhancedEP';
export { sqlEditorEnhancedEP, sqlEditorProEP } from './sqlEditorEnhancedEP';

// i18n Bridge & Creator
export * from './i18n';

// SQL Editor Contracts & Semantics
export * from './sql-editor';

// EP security gate (signature verification & trust policies)
export {
  EXTENSION_POINTS_VERSION,
  OFFICIAL_EP_PUBLIC_KEY_SPKI_B64,
  UNVERIFIED_EXTENSION_LABEL,
  checkEngineCompatibility,
  normalizeTrustedPublicKey,
  parseTrustedPublicKeys,
  readAllowUnverifiedFromEnv,
  sha256Hex,
  verifyExtensionPackage,
  verifySignatureWithPublicKey,
  type ExtensionManifest,
  type ExtensionManifestEngines,
  type ExtensionPackageFiles,
  type ExtensionSecurityConfig,
  type ExtensionSourceKind,
  type ExtensionTrustSource,
  type ExtensionVerificationFailure,
  type ExtensionVerificationResult,
  type ExtensionVerificationSuccess,
  type VerifyExtensionOptions,
} from './security';
export {
  EP_SIGNATURE_ALGORITHM,
  EP_SIGNATURE_VERSION,
  EP_SIGNED_FILE_PATHS,
  buildSignaturePayload,
  parseSignatureFile,
  type EpFileDigest,
  type EpSignatureFile,
  type EpSignedFilePath,
} from './signaturePayload';
