import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { DisplayInfo, Game, IslandFeed, OverlayStats, PlaySessionDetail, PlaySessionSummary, Settings, SgdbAsset, SgdbGame } from "./types";

function mediaSrc(path?: string | null): string | null {
  if (!path) return null;
  if (path.startsWith("http") || path.startsWith("asset") || path.startsWith("data:")) {
    return path;
  }
  try {
    return convertFileSrc(path);
  } catch {
    return path;
  }
}

function hydrate(g: Game): Game {
  return {
    ...g,
    coverUrl: mediaSrc(g.coverUrl) ?? mediaSrc(g.coverWebm) ?? mediaSrc(g.coverPoster) ?? mediaSrc(g.cover),
    bannerUrl: mediaSrc(g.bannerUrl) ?? mediaSrc(g.bannerWebm) ?? mediaSrc(g.bannerPoster) ?? mediaSrc(g.banner),
    iconUrl: mediaSrc(g.iconUrl) ?? mediaSrc(g.icon),
    coverPoster: mediaSrc(g.coverPoster) ?? g.coverPoster,
    bannerPoster: mediaSrc(g.bannerPoster) ?? g.bannerPoster,
    screenshotUrls: (g.screenshotUrls || []).map((s) => ({
      ...s,
      url: mediaSrc(s.url) || mediaSrc(s.path) || s.url,
    })),
  };
}

export const api = {
  getLibrary: async () => {
    const games = await invoke<Game[]>("get_library");
    return games.map(hydrate);
  },
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (patch: Partial<Settings>) => invoke<Settings>("set_settings", { patch }),
  addGames: async (paths: string[]) => {
    const games = await invoke<Game[]>("add_games", { paths });
    return games.map(hydrate);
  },
  updateGame: async (id: string, patch: Record<string, unknown>) => {
    const g = await invoke<Game>("update_game", { id, patch });
    return hydrate(g);
  },
  removeGame: (id: string) => invoke<boolean>("remove_game", { id }),
  reorderGames: (orderedIds: string[]) => invoke<boolean>("reorder_games", { orderedIds }),
  launchGame: async (id: string) => hydrate(await invoke<Game>("launch_game", { id })),
  recentGames: async () => (await invoke<Game[]>("recent_games")).map(hydrate),
  islandGames: async () => (await invoke<Game[]>("island_games")).map(hydrate),
  setMediaPath: async (id: string, slot: string, src: string) =>
    hydrate(await invoke<Game>("set_media_path", { id, slot, src })),
  sgdbSearch: (term: string) => invoke<SgdbGame[]>("sgdb_search", { term }),
  sgdbListAssets: (sgdbGameId: number, slot: string) =>
    invoke<SgdbAsset[]>("sgdb_list_assets", { sgdbGameId, slot }),
  sgdbApplyAsset: async (gameId: string, slot: string, url: string, mime?: string | null) =>
    hydrate(
      await invoke<Game>("sgdb_apply_asset", {
        gameId,
        slot,
        url,
        mime: mime ?? null,
      }),
    ),
  sgdbAutofetch: async (gameId: string) =>
    hydrate(await invoke<Game>("sgdb_autofetch", { gameId })),
  removeScreenshot: (id: string, path: string) =>
    invoke<boolean>("remove_screenshot", { id, path }),
  optimizeLibrary: () => invoke<number>("optimize_library"),
  processMediaJobs: () => invoke<number>("process_media_jobs"),
  toggleIsland: () => invoke<boolean>("toggle_island"),
  islandLayout: (w: number, h: number) => invoke<void>("island_layout", { w, h }),
  islandFeed: () => invoke<IslandFeed>("island_feed"),
  islandDismissNotification: (id: string) =>
    invoke<boolean>("island_dismiss_notification", { id }),
  islandClearNotifications: () => invoke<boolean>("island_clear_notifications"),
  islandMarkNotificationsRead: () => invoke<boolean>("island_mark_notifications_read"),
  islandMediaToggle: () => invoke<boolean>("island_media_toggle"),
  islandMediaNext: () => invoke<boolean>("island_media_next"),
  islandMediaPrevious: () => invoke<boolean>("island_media_previous"),
  showMain: () => invoke<void>("show_main"),
  getDisplays: () => invoke<DisplayInfo[]>("get_displays"),
  ffmpegAvailable: () => invoke<boolean>("ffmpeg_available"),
  importNebula: (force = true) => invoke<number>("import_nebula", { force }),
  openFolder: (path: string) => invoke<void>("open_folder", { path }),
  openDataDir: () => invoke<void>("open_data_dir"),
  onLibraryChanged: (cb: () => void): Promise<UnlistenFn> =>
    listen("library:changed", () => cb()),
  onSettingsChanged: (cb: (s: Settings) => void): Promise<UnlistenFn> =>
    listen<Settings>("settings:changed", (e) => cb(e.payload)),
  onScreenshot: (cb: (p: { gameId: string; path: string }) => void): Promise<UnlistenFn> =>
    listen("screenshot:captured", (e) => cb(e.payload as { gameId: string; path: string })),
  onIslandFeed: (cb: () => void): Promise<UnlistenFn> =>
    listen("island:feed", () => cb()),
  overlayStats: () => invoke<OverlayStats>("overlay_stats"),
  onOverlayStats: (cb: (s: OverlayStats) => void): Promise<UnlistenFn> =>
    listen<OverlayStats>("overlay:stats", (e) => cb(e.payload)),
  listPlaySessions: (gameId: string) =>
    invoke<PlaySessionSummary[]>("list_play_sessions", { gameId }),
  getPlaySession: (sessionId: string) =>
    invoke<PlaySessionDetail | null>("get_play_session", { sessionId }),
  deletePlaySession: (sessionId: string) =>
    invoke<boolean>("delete_play_session", { sessionId }),
  onSessionsChanged: (cb: (gameId: string) => void): Promise<UnlistenFn> =>
    listen<string>("sessions:changed", (e) => cb(e.payload)),
};
