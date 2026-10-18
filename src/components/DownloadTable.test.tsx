import { render, screen } from "@testing-library/react";
import { dl } from "../test/fixtures";
import { DownloadTable } from "./DownloadTable";

describe("DownloadTable", () => {
  it("renders real progress values and status", () => {
    const rows = [
      dl({ file_name: "ubuntu.iso", status: "downloading", total_size: 4000, downloaded: 1000, speed: 2048, eta_secs: 90, connections_active: 4 }),
      dl({ file_name: "report.pdf", status: "failed", error: { code: "not_found", message: "File not found", http_status: 404, retryable: false } }),
    ];
    render(<DownloadTable rows={rows} onOpen={() => {}} onRemove={() => {}} onNew={() => {}} />);
    expect(screen.getByText("ubuntu.iso")).toBeInTheDocument();
    expect(screen.getByText("25,0 %")).toBeInTheDocument();
    expect(screen.getByText("Lädt (4 Verbindungen)")).toBeInTheDocument();
    expect(screen.getByText("noch 1 min 30 s")).toBeInTheDocument();
    expect(screen.getByText("Datei nicht gefunden")).toBeInTheDocument();
  });

  it("shows an empty state", () => {
    render(<DownloadTable rows={[]} onOpen={() => {}} onRemove={() => {}} onNew={() => {}} />);
    expect(screen.getByText("Keine Downloads")).toBeInTheDocument();
  });
});
