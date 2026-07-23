import { LanguageToggle } from './LanguageToggle';
import { ThemeToggle } from './ThemeToggle';

export function HeaderControls() {
  return (
    <div className="flex items-center gap-0.5">
      <LanguageToggle />
      <ThemeToggle />
    </div>
  );
}
