import { render, screen } from "@testing-library/react";
import { useStore } from "../lib/store";
import type { LanguageSetting, Settings } from "../lib/types";
import { dl } from "../test/fixtures";
import { DownloadTable } from "./DownloadTable";

function setLanguage(language: LanguageSetting) {
  useStore.setState({ settings: { language } as Settings });
}

describe("DownloadTable", () => {
  const rows = () => [
    dl({ file_name: "ubuntu.iso", status: "downloading", total_size: 4000, downloaded: 1000, speed: 2048, eta_secs: 90, connections_active: 4 }),
    dl({ file_name: "report.pdf", status: "failed", error: { code: "not_found", message: "File not found", http_status: 404, retryable: false } }),
  ];

  afterEach(() => useStore.setState({ settings: null }));

  it("renders real progress values and status", () => {
    setLanguage("de");
    render(<DownloadTable rows={rows()} onOpen={() => {}} onRemove={() => {}} onNew={() => {}} />);
    expect(screen.getByText("ubuntu.iso")).toBeInTheDocument();
    expect(screen.getByText("25,0 %")).toBeInTheDocument();
    expect(screen.getByText("Lädt (4 Verbindungen)")).toBeInTheDocument();
    expect(screen.getByText("noch 1 min 30 s")).toBeInTheDocument();
    expect(screen.getByText("Datei nicht gefunden")).toBeInTheDocument();
  });

  it("renders in English", () => {
    setLanguage("en");
    render(<DownloadTable rows={rows()} onOpen={() => {}} onRemove={() => {}} onNew={() => {}} />);
    expect(screen.getByText("Progress")).toBeInTheDocument();
    expect(screen.getByText("25.0%")).toBeInTheDocument();
    expect(screen.getByText("Downloading (4 connections)")).toBeInTheDocument();
    expect(screen.getByText("1 min 30 s left")).toBeInTheDocument();
    expect(screen.getByText("File not found")).toBeInTheDocument();
  });

  it("shows an empty state", () => {
    setLanguage("de");
    render(<DownloadTable rows={[]} onOpen={() => {}} onRemove={() => {}} onNew={() => {}} />);
    expect(screen.getByText("Keine Downloads")).toBeInTheDocument();
  });

  it("follows the system language", () => {
    setLanguage("system");
    render(<DownloadTable rows={[]} onOpen={() => {}} onRemove={() => {}} onNew={() => {}} />);
    // jsdom reports navigator.language = "en-US".
    expect(screen.getByText("No downloads")).toBeInTheDocument();
  });
});
