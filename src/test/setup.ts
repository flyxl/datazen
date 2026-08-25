// Vitest 4 note: registering through "@testing-library/jest-dom/vitest"
// silently no-ops in this environment (its side-effect entry ends up with an
// empty matcher set), so every toBeInTheDocument-style assertion fails with
// "Invalid Chai property". Extending from the standalone "./matchers" export
// with the runner's own `expect` registers correctly.
import { expect } from 'vitest';
import * as jestDomMatchers from '@testing-library/jest-dom/matchers';

expect.extend(jestDomMatchers as unknown as Record<string, never>);
