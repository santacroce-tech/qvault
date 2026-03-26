export function useLicense() {
  // MIT release — no license required
  return { licensed: true, loading: false, reload: () => {} };
}
