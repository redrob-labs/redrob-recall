import type { FolderStatus } from "./types";

// A path is inside a folder when it is the folder or continues past it with a separator, so
// "library-old/a.txt" is not inside "library" (the same rule as storage.rs folder_stats).
function isInside(path: string, folder: string): boolean {
  const root = folder.replace(/[\\/]+$/, "");
  return (
    path === root || path.startsWith(root + "/") || path.startsWith(root + "\\")
  );
}

/**
 * True when the deepest library folder holding `path` is gone. Its files stay indexed -- an
 * unplugged drive usually comes back, and dropping them would mean reading it all again -- but a
 * result from it cannot be opened, and the screen must say so.
 */
export function inMissingFolder(
  path: string,
  folders: FolderStatus[],
): boolean {
  let owner: FolderStatus | undefined;
  for (const folder of folders) {
    if (
      isInside(path, folder.path) &&
      (!owner || folder.path.length > owner.path.length)
    ) {
      owner = folder;
    }
  }
  return owner ? !owner.present : false;
}
