import clsx from "clsx";
import { File, FileArchive, FileText, Film, Image, Music, Package } from "lucide-react";

const MAP: Record<string, { icon: typeof File; color: string }> = {
  video: { icon: Film, color: "text-violet-500" },
  music: { icon: Music, color: "text-pink-500" },
  documents: { icon: FileText, color: "text-sky-600" },
  programs: { icon: Package, color: "text-amber-600" },
  archives: { icon: FileArchive, color: "text-emerald-600" },
  images: { icon: Image, color: "text-teal-500" },
};

export function FileIcon({ category, size = 18, className }: { category: string; size?: number; className?: string }) {
  const entry = MAP[category] ?? { icon: File, color: "text-muted" };
  const Icon = entry.icon;
  return <Icon size={size} className={clsx(entry.color, "shrink-0", className)} aria-hidden />;
}
