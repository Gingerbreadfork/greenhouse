export async function open(opts?: { directory?: boolean; multiple?: boolean }) {
  if (opts?.directory) return '/home/you/Media/Open movies';
  return opts?.multiple ? ['/home/you/Downloads/example.torrent'] : '/home/you/Downloads/example.torrent';
}
export async function message() {}
export async function confirm() {
  return true;
}
