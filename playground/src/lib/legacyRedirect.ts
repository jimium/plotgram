/** `/playground` → `/editor`，保留 query 与 hash。 */
export function redirectLegacyPlaygroundPath(): void {
  const { pathname, search, hash } = window.location;
  if (!pathname.startsWith('/playground')) {
    return;
  }
  const suffix = pathname.slice('/playground'.length) || '/';
  const targetPath = suffix === '/' ? '/editor/' : `/editor${suffix}`;
  window.location.replace(`${targetPath}${search}${hash}`);
}
