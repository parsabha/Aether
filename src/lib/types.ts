export type MediaKind = "image" | "video" | "gif";

export interface Screenshot {
  path: string;
  url: string;
}

export interface Game {
  id: string;
  name: string;
  exePath?: string | null;
  args: string;
  cwd?: string | null;
  cover?: string | null;
  coverWebm?: string | null;
  coverPoster?: string | null;
  banner?: string | null;
  bannerWebm?: string | null;
  bannerPoster?: string | null;
  icon?: string | null;
  description: string;
  category: string;
  tags: string[];
  favorite: boolean;
  accent?: string | null;
  sortOrder: number;
  playtimeMs: number;
  launchCount: number;
  lastPlayed?: number | null;
  addedAt: number;
  deletedAt?: number | null;
  coverUrl?: string | null;
  coverKind: MediaKind;
  bannerUrl?: string | null;
  bannerKind: MediaKind;
  iconUrl?: string | null;
  screenshotUrls: Screenshot[];
  running: boolean;
  missing: boolean;
}

export interface Settings {
  accent: string;
  sort: string;
  view: string;
  islandEnabled: boolean;
  islandOnTop: boolean;
  islandDisplay?: number | null;
  blur: string;
  reduceMotion: boolean;
  launchOnStartup: boolean;
  startMinimized: boolean;
  launchAsAdmin: boolean;
  screenshotHotkeyEnabled: boolean;
  screenshotHotkey: string;
  nebulaImported: boolean;
}

export interface DisplayInfo {
  id: number;
  name: string;
  width: number;
  height: number;
}

export interface IslandNotification {
  id: string;
  kind: string;
  title: string;
  body: string;
  gameId?: string | null;
  app?: string | null;
  createdAt: number;
  read: boolean;
}

export interface IslandMedia {
  title: string;
  artist: string;
  album: string;
  appName: string;
  status: "playing" | "paused" | "stopped" | string;
  positionMs: number;
  durationMs: number;
  thumbnailDataUrl?: string | null;
  source: "smtc" | "audio" | string;
}

export interface IslandFeed {
  media?: IslandMedia | null;
  notifications: IslandNotification[];
  systemAccess: boolean;
}
