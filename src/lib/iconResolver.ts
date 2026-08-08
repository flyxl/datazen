export type IconKind = 'lucide' | 'url' | 'placeholder';

export type ResolvedIcon =
  | { kind: 'lucide'; name: string }
  | { kind: 'url'; href: string }
  | { kind: 'placeholder'; label: string; bgClass: string };

export type IconSourceMap = Record<string, string>;

export interface IconResolver {
  resolve(semanticId: string): ResolvedIcon;
}

export interface CreateIconResolverOptions {
  packIcons: IconSourceMap;
  driverIcons: IconSourceMap;
  lucideById: Record<string, string>;
  placeholderForDb: (dbType: string) => { label: string; bgClass: string };
}

const UI_PLACEHOLDER: ResolvedIcon = {
  kind: 'placeholder',
  label: '?',
  bgClass: 'bg-slate-600',
};

function isDbIconId(id: string): boolean {
  return id.startsWith('db.');
}

export function createIconResolver(opts: CreateIconResolverOptions): IconResolver {
  const { packIcons, driverIcons, lucideById, placeholderForDb } = opts;

  return {
    resolve(semanticId: string): ResolvedIcon {
      const packUrl = packIcons[semanticId];
      if (packUrl) {
        return { kind: 'url', href: packUrl };
      }

      if (isDbIconId(semanticId)) {
        const driverUrl = driverIcons[semanticId];
        if (driverUrl) {
          return { kind: 'url', href: driverUrl };
        }
        const dbType = semanticId.slice('db.'.length);
        return { kind: 'placeholder', ...placeholderForDb(dbType) };
      }

      const lucideName = lucideById[semanticId];
      if (lucideName) {
        return { kind: 'lucide', name: lucideName };
      }

      return UI_PLACEHOLDER;
    },
  };
}

let activeIconResolver: IconResolver = createIconResolver({
  packIcons: {},
  driverIcons: {},
  lucideById: {},
  placeholderForDb: (dbType) => ({ label: dbType.slice(0, 2), bgClass: 'bg-slate-600' }),
});

export function setActiveIconResolver(resolver: IconResolver): void {
  activeIconResolver = resolver;
}

export function getActiveIconResolver(): IconResolver {
  return activeIconResolver;
}
