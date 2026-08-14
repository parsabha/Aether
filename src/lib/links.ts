import distribution from "./distribution.json";

export const LINKS = distribution;

export function primarySupportUrl(): string | null {
  const url =
    LINKS.donateUrl || LINKS.koFiUrl || LINKS.itchUrl || LINKS.repoUrl;
  return url.trim() ? url.trim() : null;
}
