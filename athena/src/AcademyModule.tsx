import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ArrowLeft, Printer } from "lucide-react";
import { academyZimOffersForUi, type AcademyZimOfferForUi } from "./shared/academyZimCatalog";
import { AcademyDownloader } from "./features/academy/AcademyDownloader";

const KIWIX_ORIGIN = "http://localhost:8080";

const ZIM_OFFERS: AcademyZimOfferForUi[] = academyZimOffersForUi();

interface AcademyLesson {
  id: string;
  title: string;
  zimNameHint: string;
  articleUrl: string;
}

interface AcademySubject {
  id: string;
  label: string;
  lessons: AcademyLesson[];
}

interface AcademyAgeGroup {
  id: string;
  label: string;
  subjects: AcademySubject[];
}

interface AcademyCurriculumRoot {
  ageGroups: AcademyAgeGroup[];
}

interface AcademyLessonHtml {
  title: string;
  html: string;
}

type MainTab = "library" | "downloads" | "curriculum";

type Nav =
  | { level: "subjects" }
  | { level: "grades"; subjectId: string; subjectLabel: string }
  | { level: "units"; subjectId: string; subjectLabel: string; grade: AcademyAgeGroup }
  | { level: "unit"; subjectLabel: string; gradeLabel: string; lesson: AcademyLesson };

interface ZimDownloadProgressPayload {
  downloadId: string;
  bytesDownloaded: number;
  totalBytes: number | null;
  percent: number | null;
}

interface ZimDownloadCompletePayload {
  downloadId: string;
  path: string;
}

interface ZimDownloadErrorPayload {
  downloadId: string;
  message: string;
}

function makeTeacherGuideHtml(args: { subjectLabel: string; gradeLabel: string; lessonTitle: string; articleUrl: string }) {
  const { subjectLabel, gradeLabel, lessonTitle, articleUrl } = args;
  const safeUrl = articleUrl || "";
  return `<!doctype html>
<html>
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Teacher Guide — ${lessonTitle}</title>
    <style>
      body { font-family: system-ui, -apple-system, Segoe UI, Roboto, sans-serif; padding: 16px; line-height: 1.4; }
      h1 { margin: 0 0 8px; font-size: 18px; }
      h2 { margin: 16px 0 6px; font-size: 14px; }
      .meta { color: #555; font-size: 12px; margin-bottom: 12px; }
      code { background: #f3f3f3; padding: 1px 4px; border-radius: 4px; }
      ul { margin: 8px 0 0; padding-left: 18px; }
      .box { border: 1px solid #ddd; border-radius: 10px; padding: 12px; margin: 12px 0; }
    </style>
  </head>
  <body>
    <h1>${lessonTitle}</h1>
    <div class="meta">${subjectLabel} · ${gradeLabel}</div>
    <div class="box">
      <h2>Instructor intent</h2>
      <ul>
        <li>Establish key terms before reading.</li>
        <li>Have the student summarize each section aloud.</li>
        <li>End with a practical exercise or short written response.</li>
      </ul>
    </div>
    <div class="box">
      <h2>Discussion prompts</h2>
      <ul>
        <li>What is the main idea of this lesson?</li>
        <li>Which part would you apply in a real-world scenario?</li>
        <li>What would fail first if conditions were harsher?</li>
      </ul>
    </div>
    <div class="box">
      <h2>Source</h2>
      <div class="meta">Backed by your offline ZIM library. Curriculum URL hint: <code>${safeUrl}</code></div>
    </div>
  </body>
</html>`;
}

type DownloadRowState =
  | { status: "idle" }
  | { status: "downloading"; percent: number; bytesDownloaded: number; totalBytes: number | null }
  | { status: "done" }
  | { status: "error"; message: string };

export function AcademyModule() {
  const [mainTab, setMainTab] = useState<MainTab>("library");
  const [iframeKey, setIframeKey] = useState(0);
  const [curriculum, setCurriculum] = useState<AcademyCurriculumRoot | null>(null);
  const [nav, setNav] = useState<Nav>({ level: "subjects" });
  const [loadErr, setLoadErr] = useState<string | null>(null);
  const [studentHtml, setStudentHtml] = useState<string | null>(null);
  const [studentTitle, setStudentTitle] = useState<string>("");
  const [loadingUnit, setLoadingUnit] = useState(false);
  const [downloadRows, setDownloadRows] = useState<Record<string, DownloadRowState>>({});

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const c = await invoke<AcademyCurriculumRoot>("academy_get_curriculum");
        if (!cancelled) setCurriculum(c);
      } catch (e) {
        if (!cancelled) setLoadErr(e != null ? String(e) : "Could not load curriculum.");
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    let disposed = false;
    const unsubs: Array<() => void> = [];

    void (async () => {
      const u1 = await listen<ZimDownloadProgressPayload>("zim_download_progress", (ev) => {
        const pld = ev.payload;
        setDownloadRows((prev) => ({
          ...prev,
          [pld.downloadId]: {
            status: "downloading",
            percent:
              pld.percent ??
              (pld.totalBytes ? Math.min(100, (pld.bytesDownloaded / pld.totalBytes) * 100) : 0),
            bytesDownloaded: pld.bytesDownloaded,
            totalBytes: pld.totalBytes,
          },
        }));
      });
      if (disposed) {
        u1();
        return;
      }
      unsubs.push(u1);

      const u2 = await listen<ZimDownloadCompletePayload>("zim_download_complete", (ev) => {
        const pld = ev.payload;
        setDownloadRows((prev) => ({ ...prev, [pld.downloadId]: { status: "done" } }));
        setIframeKey((k) => k + 1);
      });
      if (disposed) {
        u2();
        return;
      }
      unsubs.push(u2);

      const u3 = await listen<ZimDownloadErrorPayload>("zim_download_error", (ev) => {
        const pld = ev.payload;
        setDownloadRows((prev) => ({
          ...prev,
          [pld.downloadId]: { status: "error", message: pld.message },
        }));
      });
      if (disposed) {
        u3();
        return;
      }
      unsubs.push(u3);
    })();

    return () => {
      disposed = true;
      unsubs.forEach((u) => u());
    };
  }, []);

  const subjects = useMemo(() => {
    if (!curriculum) return [];
    const map = new Map<string, { id: string; label: string }>();
    for (const g of curriculum.ageGroups) {
      for (const s of g.subjects) {
        if (!map.has(s.id)) map.set(s.id, { id: s.id, label: s.label });
      }
    }
    return Array.from(map.values()).sort((a, b) => a.label.localeCompare(b.label));
  }, [curriculum]);

  const gradesForSubject = useCallback(
    (subjectId: string) => {
      if (!curriculum) return [];
      return curriculum.ageGroups
        .filter((g) => g.subjects.some((s) => s.id === subjectId))
        .sort((a, b) => a.label.localeCompare(b.label));
    },
    [curriculum],
  );

  const unitsForSubjectGrade = useCallback((subjectId: string, grade: AcademyAgeGroup) => {
    const subj = grade.subjects.find((s) => s.id === subjectId);
    return subj?.lessons ?? [];
  }, []);

  const openUnit = useCallback(async (subjectLabel: string, gradeLabel: string, lesson: AcademyLesson) => {
    setNav({ level: "unit", subjectLabel, gradeLabel, lesson });
    setStudentHtml(null);
    setStudentTitle(lesson.title);
    setLoadingUnit(true);
    setLoadErr(null);
    try {
      const res = await invoke<AcademyLessonHtml>("academy_get_lesson_html", { args: { lessonId: lesson.id } });
      setStudentTitle(res.title);
      setStudentHtml(res.html);
    } catch (e) {
      setLoadErr(e != null ? String(e) : "Could not load lesson.");
    } finally {
      setLoadingUnit(false);
    }
  }, []);

  const teacherHtml = useMemo(() => {
    if (nav.level !== "unit") return null;
    return makeTeacherGuideHtml({
      subjectLabel: nav.subjectLabel,
      gradeLabel: nav.gradeLabel,
      lessonTitle: studentTitle || nav.lesson.title,
      articleUrl: nav.lesson.articleUrl,
    });
  }, [nav, studentTitle]);

  const printPacket = useCallback(() => {
    if (nav.level !== "unit") return;
    const student = studentHtml ?? "<p>Student reader not loaded.</p>";
    const teacher = teacherHtml ?? "<p>Teacher guide not available.</p>";
    const title = studentTitle || nav.lesson.title;

    const win = window.open("", "_blank", "noopener,noreferrer,width=980,height=720");
    if (!win) return;
    win.document.open();
    win.document.write(`<!doctype html>
<html>
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Lesson Packet — ${title}</title>
    <style>
      @media print { .no-print { display: none; } }
      body { font-family: system-ui, -apple-system, Segoe UI, Roboto, sans-serif; margin: 0; }
      header { padding: 12px 16px; border-bottom: 1px solid #ddd; }
      .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; padding: 12px 16px; }
      .pane { border: 1px solid #ddd; border-radius: 10px; overflow: hidden; }
      .pane h2 { margin: 0; padding: 10px 12px; font-size: 14px; background: #f6f6f6; border-bottom: 1px solid #ddd; }
      .pane .body { padding: 12px; }
      iframe, .body { width: 100%; }
    </style>
  </head>
  <body>
    <header>
      <div style="font-weight: 700">${title}</div>
      <div style="color: #555; font-size: 12px">${nav.subjectLabel} · ${nav.gradeLabel}</div>
      <div class="no-print" style="margin-top: 10px">
        <button onclick="window.print()">Print</button>
      </div>
    </header>
    <div class="grid">
      <section class="pane">
        <h2>Student Reader</h2>
        <div class="body">${student}</div>
      </section>
      <section class="pane">
        <h2>Teacher Lecture Guide</h2>
        <div class="body">${teacher}</div>
      </section>
    </div>
  </body>
</html>`);
    win.document.close();
    win.focus();
    win.print();
  }, [nav, studentHtml, teacherHtml, studentTitle]);

  const startZimDownload = useCallback(async (offer: AcademyZimOfferForUi) => {
    setDownloadRows((prev) => ({ ...prev, [offer.id]: { status: "idle" } }));
    try {
      await invoke("download_zim", {
        args: {
          url: offer.url,
          fileName: offer.fileName,
          downloadId: offer.id,
        },
      });
    } catch (e) {
      setDownloadRows((prev) => ({
        ...prev,
        [offer.id]: { status: "error", message: e != null ? String(e) : "Download failed to start." },
      }));
    }
  }, []);

  const tabBar = (
    <div style={{ display: "flex", flexWrap: "wrap", gap: "0.35rem", alignItems: "center" }}>
      <button type="button" className={mainTab === "library" ? "glass-action-btn" : "secondary"} onClick={() => setMainTab("library")}>
        Kiwix library
      </button>
      <button type="button" className={mainTab === "downloads" ? "glass-action-btn" : "secondary"} onClick={() => setMainTab("downloads")}>
        ZIM downloads
      </button>
      <button type="button" className={mainTab === "curriculum" ? "glass-action-btn" : "secondary"} onClick={() => setMainTab("curriculum")}>
        Curriculum lessons
      </button>
    </div>
  );

  if (mainTab === "library") {
    return (
      <div
        className="glass-panel app-view-placeholder"
        style={{ display: "flex", flexDirection: "column", gap: "0.75rem", padding: "1rem", minHeight: "min(90vh, 720px)", flex: 1 }}
      >
        <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", alignItems: "center", justifyContent: "space-between" }}>
          <h2 className="app-view-placeholder-title" style={{ margin: 0 }}>
            Academy
          </h2>
          {tabBar}
        </div>
        <p className="text-muted" style={{ margin: 0 }}>
          Offline ZIMs in <code>payload/ZIM_Archives</code> are served at{" "}
          <code>{KIWIX_ORIGIN}</code> by the bundled <code>kiwix-serve</code> sidecar. Use the native Kiwix UI below to browse.
        </p>
        <div className="glass-panel" style={{ flex: 1, minHeight: "55vh", padding: 0, overflow: "hidden", display: "flex", flexDirection: "column" }}>
          <iframe
            key={iframeKey}
            title="Kiwix library"
            src={`${KIWIX_ORIGIN}/`}
            style={{ flex: 1, width: "100%", minHeight: "55vh", border: "none", background: "#111" }}
            sandbox="allow-same-origin allow-scripts allow-forms allow-popups allow-modals allow-downloads"
          />
        </div>
      </div>
    );
  }

  if (mainTab === "downloads") {
    return (
      <div className="glass-panel app-view-placeholder" style={{ display: "grid", gap: "0.75rem", padding: "1rem" }}>
        <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", alignItems: "center", justifyContent: "space-between" }}>
          <h2 className="app-view-placeholder-title" style={{ margin: 0 }}>
            ZIM downloads
          </h2>
          {tabBar}
        </div>
        <p className="text-muted" style={{ margin: 0 }}>
          Files save into <code>payload/ZIM_Archives</code>. Large downloads can take hours; keep the app open and on power.
        </p>
        <div className="glass-panel" style={{ padding: 0 }}>
          <AcademyDownloader />
        </div>
        <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "grid", gap: "0.75rem" }}>
          {ZIM_OFFERS.map((offer) => {
            const row = downloadRows[offer.id];
            const pct =
              row?.status === "downloading"
                ? row.percent
                : row?.status === "done"
                  ? 100
                  : 0;
            return (
              <li key={offer.id} className="glass-panel" style={{ padding: "0.75rem", display: "grid", gap: "0.5rem" }}>
                <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", justifyContent: "space-between", alignItems: "flex-start" }}>
                  <div>
                    <div style={{ fontWeight: 700 }}>{offer.label}</div>
                    <div className="text-muted" style={{ fontSize: "0.9rem", marginTop: "0.25rem" }}>
                      {offer.description} · Approx. {offer.approxSize}
                    </div>
                  </div>
                  <button
                    type="button"
                    className="glass-action-btn"
                    disabled={row?.status === "downloading"}
                    onClick={() => void startZimDownload(offer)}
                  >
                    {row?.status === "downloading" ? "Downloading…" : row?.status === "done" ? "Download again" : "Download"}
                  </button>
                </div>
                <div
                  role="progressbar"
                  aria-valuemin={0}
                  aria-valuemax={100}
                  aria-valuenow={Math.round(pct)}
                  style={{
                    height: "10px",
                    borderRadius: "6px",
                    background: "var(--glass-border, rgba(255,255,255,0.12))",
                    overflow: "hidden",
                  }}
                >
                  <div
                    style={{
                      width: `${Math.min(100, Math.max(0, pct))}%`,
                      height: "100%",
                      background: "var(--accent, #4f8cff)",
                      transition: "width 0.2s ease-out",
                    }}
                  />
                </div>
                {row?.status === "downloading" ? (
                  <div className="text-muted" style={{ fontSize: "0.85rem" }}>
                    {row.totalBytes != null
                      ? `${(row.bytesDownloaded / 1_000_000).toFixed(1)} / ${(row.totalBytes / 1_000_000).toFixed(1)} MB`
                      : `${(row.bytesDownloaded / 1_000_000).toFixed(1)} MB downloaded`}
                  </div>
                ) : null}
                {row?.status === "error" ? (
                  <p className="error" role="alert" style={{ margin: 0 }}>
                    {row.message}
                  </p>
                ) : null}
                {row?.status === "done" ? (
                  <p className="text-muted" style={{ margin: 0 }}>
                    Finished. Open <strong>Kiwix library</strong> to browse (the server reloads automatically).
                  </p>
                ) : null}
              </li>
            );
          })}
        </ul>
      </div>
    );
  }

  if (!curriculum) {
    return (
      <div className="glass-panel app-view-placeholder" style={{ padding: "1rem" }}>
        <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", alignItems: "center", justifyContent: "space-between" }}>
          <h2 className="app-view-placeholder-title">Academy</h2>
          {tabBar}
        </div>
        <p className="text-muted">{loadErr ?? "Loading curriculum…"}</p>
      </div>
    );
  }

  if (nav.level === "subjects") {
    return (
      <div className="glass-panel app-view-placeholder" style={{ display: "grid", gap: "0.75rem", padding: "1rem" }}>
        <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", alignItems: "center", justifyContent: "space-between" }}>
          <h2 className="app-view-placeholder-title" style={{ margin: 0 }}>
            Academy
          </h2>
          {tabBar}
        </div>
        <p className="text-muted" style={{ margin: 0 }}>
          Structured lessons read articles from your local ZIM files (same discovery rules as the library). For free browsing, use{" "}
          <strong>Kiwix library</strong>.
        </p>
        <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "grid", gap: "0.5rem" }}>
          {subjects.map((s) => (
            <li key={s.id}>
              <button
                type="button"
                className="glass-action-btn"
                style={{ width: "100%", justifyContent: "flex-start" }}
                onClick={() => setNav({ level: "grades", subjectId: s.id, subjectLabel: s.label })}
              >
                {s.label}
              </button>
            </li>
          ))}
        </ul>
      </div>
    );
  }

  if (nav.level === "grades") {
    const grades = gradesForSubject(nav.subjectId);
    return (
      <div className="glass-panel app-view-placeholder" style={{ display: "grid", gap: "0.75rem", padding: "1rem" }}>
        <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", alignItems: "center", justifyContent: "space-between" }}>
          <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", alignItems: "center" }}>
            <button type="button" className="secondary" onClick={() => setNav({ level: "subjects" })}>
              <ArrowLeft size={16} strokeWidth={2} aria-hidden style={{ verticalAlign: "middle", marginRight: "0.35rem" }} />
              Subjects
            </button>
            <span style={{ fontWeight: 700 }}>{nav.subjectLabel}</span>
          </div>
          {tabBar}
        </div>
        <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "grid", gap: "0.5rem" }}>
          {grades.map((g) => (
            <li key={g.id}>
              <button
                type="button"
                className="glass-action-btn"
                style={{ width: "100%", justifyContent: "flex-start" }}
                onClick={() => setNav({ level: "units", subjectId: nav.subjectId, subjectLabel: nav.subjectLabel, grade: g })}
              >
                {g.label}
              </button>
            </li>
          ))}
        </ul>
      </div>
    );
  }

  if (nav.level === "units") {
    const units = unitsForSubjectGrade(nav.subjectId, nav.grade);
    return (
      <div className="glass-panel app-view-placeholder" style={{ display: "grid", gap: "0.75rem", padding: "1rem" }}>
        <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", alignItems: "center", justifyContent: "space-between" }}>
          <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", alignItems: "center" }}>
            <button
              type="button"
              className="secondary"
              onClick={() => setNav({ level: "grades", subjectId: nav.subjectId, subjectLabel: nav.subjectLabel })}
            >
              <ArrowLeft size={16} strokeWidth={2} aria-hidden style={{ verticalAlign: "middle", marginRight: "0.35rem" }} />
              Grades
            </button>
            <span style={{ fontWeight: 700 }}>
              {nav.subjectLabel} · {nav.grade.label}
            </span>
          </div>
          {tabBar}
        </div>
        <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "grid", gap: "0.5rem" }}>
          {units.map((u) => (
            <li key={u.id}>
              <button
                type="button"
                className="glass-action-btn"
                style={{ width: "100%", justifyContent: "flex-start" }}
                onClick={() => void openUnit(nav.subjectLabel, nav.grade.label, u)}
              >
                {u.title}
              </button>
            </li>
          ))}
        </ul>
      </div>
    );
  }

  return (
    <div className="glass-panel app-view-placeholder" style={{ display: "grid", gap: "0.75rem", padding: "1rem" }}>
      <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", alignItems: "center", justifyContent: "space-between" }}>
        <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", alignItems: "center" }}>
          <button
            type="button"
            className="secondary"
            onClick={() =>
              setNav((prev) => {
                if (prev.level !== "unit") return prev;
                const subject = subjects.find((s) => s.label === prev.subjectLabel);
                const subjectId = subject?.id ?? "";
                if (!subjectId || !curriculum) return { level: "subjects" };
                const grade = curriculum.ageGroups.find((g) => g.label === prev.gradeLabel) ?? curriculum.ageGroups[0];
                return { level: "units", subjectId, subjectLabel: prev.subjectLabel, grade };
              })
            }
            disabled={loadingUnit}
          >
            <ArrowLeft size={16} strokeWidth={2} aria-hidden style={{ verticalAlign: "middle", marginRight: "0.35rem" }} />
            Units
          </button>
          <span style={{ fontWeight: 700 }}>{studentTitle}</span>
        </div>
        <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", alignItems: "center" }}>
          {tabBar}
          <button type="button" className="secondary" onClick={() => printPacket()} disabled={loadingUnit}>
            <Printer size={16} strokeWidth={2} aria-hidden style={{ verticalAlign: "middle", marginRight: "0.35rem" }} />
            Print packet
          </button>
        </div>
      </div>

      {loadErr ? (
        <p className="error" role="alert">
          {loadErr}
        </p>
      ) : null}

      <div className="glass-panel" style={{ padding: "0.75rem", display: "grid", gap: "0.75rem" }}>
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "0.75rem" }}>
          <div className="glass-panel" style={{ padding: 0, overflow: "hidden", minHeight: "55vh" }}>
            <div style={{ padding: "0.5rem 0.75rem", fontWeight: 700, borderBottom: "1px solid var(--glass-border, rgba(255,255,255,0.12))" }}>
              Student Reader
            </div>
            {loadingUnit ? (
              <p className="text-muted" style={{ padding: "1rem" }}>
                Loading…
              </p>
            ) : studentHtml ? (
              <iframe
                title={`Student Reader: ${studentTitle}`}
                sandbox=""
                srcDoc={studentHtml}
                style={{ width: "100%", minHeight: "55vh", border: "none", display: "block", background: "var(--surface-solid, #fff)" }}
              />
            ) : (
              <p className="text-muted" style={{ padding: "1rem" }}>
                No content.
              </p>
            )}
          </div>

          <div className="glass-panel" style={{ padding: 0, overflow: "hidden", minHeight: "55vh" }}>
            <div style={{ padding: "0.5rem 0.75rem", fontWeight: 700, borderBottom: "1px solid var(--glass-border, rgba(255,255,255,0.12))" }}>
              Teacher Lecture Guide
            </div>
            {teacherHtml ? (
              <iframe
                title={`Teacher Guide: ${studentTitle}`}
                sandbox=""
                srcDoc={teacherHtml}
                style={{ width: "100%", minHeight: "55vh", border: "none", display: "block", background: "var(--surface-solid, #fff)" }}
              />
            ) : (
              <p className="text-muted" style={{ padding: "1rem" }}>
                No guide.
              </p>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
