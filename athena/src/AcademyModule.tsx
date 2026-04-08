import { useCallback, useEffect, useMemo, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { ArrowLeft, Printer } from "lucide-react";

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

interface AcademyPdfUnit {
  id: string;
  title: string;
  grade: number;
  subject: string;
  tags: string[];
  readerPath: string | null;
  lecturePath: string | null;
}

type Nav =
  | { level: "subjects" }
  | { level: "grades"; subjectId: string; subjectLabel: string }
  | { level: "units"; subjectId: string; subjectLabel: string; grade: AcademyAgeGroup }
  | { level: "unit"; subjectLabel: string; gradeLabel: string; lesson: AcademyLesson };

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

export function AcademyModule() {
  const [curriculum, setCurriculum] = useState<AcademyCurriculumRoot | null>(null);
  const [nav, setNav] = useState<Nav>({ level: "subjects" });
  const [loadErr, setLoadErr] = useState<string | null>(null);
  const [studentHtml, setStudentHtml] = useState<string | null>(null);
  const [studentTitle, setStudentTitle] = useState<string>("");
  const [loadingUnit, setLoadingUnit] = useState(false);
  const [source, setSource] = useState<"zim" | "pdf">("zim");
  const [pdfUnits, setPdfUnits] = useState<AcademyPdfUnit[] | null>(null);
  const [pdfNav, setPdfNav] = useState<AcademyPdfUnit | null>(null);
  const [pdfErr, setPdfErr] = useState<string | null>(null);

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
    let cancelled = false;
    (async () => {
      try {
        const list = await invoke<AcademyPdfUnit[]>("academy_list_pdf_units");
        if (!cancelled) setPdfUnits(list);
      } catch (e) {
        if (!cancelled) {
          setPdfErr(e != null ? String(e) : "Could not load PDF curriculum.");
          setPdfUnits([]);
        }
      }
    })();
    return () => {
      cancelled = true;
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

  if (source === "zim" && !curriculum) {
    return (
      <div className="glass-panel app-view-placeholder" style={{ padding: "1rem" }}>
        <h2 className="app-view-placeholder-title">Academy</h2>
        <p className="text-muted">{loadErr ?? "Loading curriculum…"}</p>
      </div>
    );
  }

  if (source === "pdf") {
    if (pdfUnits === null) {
      return (
        <div className="glass-panel app-view-placeholder" style={{ padding: "1rem" }}>
          <h2 className="app-view-placeholder-title">Academy</h2>
          <p className="text-muted">Loading PDF archive…</p>
        </div>
      );
    }
    if (pdfNav) {
      const r = pdfNav.readerPath ? convertFileSrc(pdfNav.readerPath) : "";
      const l = pdfNav.lecturePath ? convertFileSrc(pdfNav.lecturePath) : "";
      return (
        <div className="glass-panel app-view-placeholder" style={{ display: "grid", gap: "0.75rem", padding: "1rem" }}>
          <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", alignItems: "center", justifyContent: "space-between" }}>
            <div style={{ display: "flex", gap: "0.5rem", alignItems: "center" }}>
              <button type="button" className="secondary" onClick={() => setPdfNav(null)}>
                <ArrowLeft size={16} strokeWidth={2} aria-hidden style={{ verticalAlign: "middle", marginRight: "0.35rem" }} />
                Units
              </button>
              <span style={{ fontWeight: 700 }}>{pdfNav.title}</span>
            </div>
            <div style={{ display: "flex", gap: "0.35rem" }}>
              <button type="button" className="secondary" onClick={() => { setSource("zim"); setPdfNav(null); }}>
                ZIM lessons
              </button>
              <button type="button" className="glass-action-btn" onClick={() => setSource("pdf")}>
                PDF archive
              </button>
            </div>
          </div>
          {pdfErr ? <p className="error">{pdfErr}</p> : null}
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "0.75rem" }}>
            <div className="glass-panel" style={{ padding: 0, overflow: "hidden", minHeight: "55vh" }}>
              <div style={{ padding: "0.5rem 0.75rem", fontWeight: 700, borderBottom: "1px solid var(--glass-border, rgba(255,255,255,0.12))" }}>Student Reader</div>
              {r ? (
                <iframe title="Student PDF" src={r} style={{ width: "100%", minHeight: "55vh", border: "none", display: "block", background: "#222" }} />
              ) : (
                <p className="text-muted" style={{ padding: "1rem" }}>Add <code>reader.pdf</code> to this unit folder.</p>
              )}
            </div>
            <div className="glass-panel" style={{ padding: 0, overflow: "hidden", minHeight: "55vh" }}>
              <div style={{ padding: "0.5rem 0.75rem", fontWeight: 700, borderBottom: "1px solid var(--glass-border, rgba(255,255,255,0.12))" }}>Teacher Lecture Guide</div>
              {l ? (
                <iframe title="Teacher PDF" src={l} style={{ width: "100%", minHeight: "55vh", border: "none", display: "block", background: "#222" }} />
              ) : (
                <p className="text-muted" style={{ padding: "1rem" }}>Add <code>lecture_guide.pdf</code> to this unit folder.</p>
              )}
            </div>
          </div>
        </div>
      );
    }
    return (
      <div className="glass-panel app-view-placeholder" style={{ display: "grid", gap: "0.75rem", padding: "1rem" }}>
        <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", alignItems: "center", justifyContent: "space-between" }}>
          <h2 className="app-view-placeholder-title" style={{ margin: 0 }}>Academy</h2>
          <div style={{ display: "flex", gap: "0.35rem" }}>
            <button type="button" className="secondary" onClick={() => { setSource("zim"); setPdfNav(null); }}>
              ZIM lessons
            </button>
            <button type="button" className="glass-action-btn" onClick={() => setSource("pdf")}>
              PDF archive
            </button>
          </div>
        </div>
        {pdfErr ? <p className="error">{pdfErr}</p> : null}
        <p className="text-muted" style={{ margin: 0 }}>
          Bundled units under <code>payload/Academy</code>. Place <code>reader.pdf</code> and <code>lecture_guide.pdf</code> in each unit directory.
        </p>
        <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "grid", gap: "0.5rem" }}>
          {pdfUnits.length === 0 ? (
            <li className="text-muted">No units found. Add folders with metadata.json under payload/Academy.</li>
          ) : (
            pdfUnits.map((u) => (
              <li key={u.id}>
                <button type="button" className="glass-action-btn" style={{ width: "100%", justifyContent: "flex-start" }} onClick={() => setPdfNav(u)}>
                  <span style={{ fontWeight: 600 }}>{u.title}</span>
                  <span className="text-muted" style={{ marginLeft: "0.5rem" }}>
                    {u.subject} · Grade {u.grade}
                  </span>
                </button>
              </li>
            ))
          )}
        </ul>
      </div>
    );
  }

  if (nav.level === "subjects") {
    return (
      <div className="glass-panel app-view-placeholder" style={{ display: "grid", gap: "0.75rem", padding: "1rem" }}>
        <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem", alignItems: "center", justifyContent: "space-between" }}>
          <h2 className="app-view-placeholder-title" style={{ margin: 0 }}>Academy</h2>
          <div style={{ display: "flex", gap: "0.35rem" }}>
            <button type="button" className="glass-action-btn" onClick={() => { setSource("zim"); setPdfNav(null); }}>
              ZIM lessons
            </button>
            <button type="button" className="secondary" onClick={() => { setSource("pdf"); setNav({ level: "subjects" }); }}>
              PDF archive
            </button>
          </div>
        </div>
        <p className="text-muted" style={{ margin: 0 }}>
          Select a subject, then grade, then unit. Lessons are pulled from your offline ZIM library.
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
          <div style={{ display: "flex", gap: "0.35rem" }}>
            <button type="button" className="glass-action-btn" onClick={() => { setSource("zim"); setPdfNav(null); }}>ZIM lessons</button>
            <button type="button" className="secondary" onClick={() => { setSource("pdf"); setNav({ level: "subjects" }); }}>PDF archive</button>
          </div>
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
            <button type="button" className="secondary" onClick={() => setNav({ level: "grades", subjectId: nav.subjectId, subjectLabel: nav.subjectLabel })}>
              <ArrowLeft size={16} strokeWidth={2} aria-hidden style={{ verticalAlign: "middle", marginRight: "0.35rem" }} />
              Grades
            </button>
            <span style={{ fontWeight: 700 }}>
              {nav.subjectLabel} · {nav.grade.label}
            </span>
          </div>
          <div style={{ display: "flex", gap: "0.35rem" }}>
            <button type="button" className="glass-action-btn" onClick={() => { setSource("zim"); setPdfNav(null); }}>ZIM lessons</button>
            <button type="button" className="secondary" onClick={() => { setSource("pdf"); setNav({ level: "subjects" }); }}>PDF archive</button>
          </div>
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

  // Unit view
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
          <div style={{ display: "flex", gap: "0.35rem" }}>
            <button type="button" className="glass-action-btn" onClick={() => { setSource("zim"); setPdfNav(null); }}>ZIM lessons</button>
            <button type="button" className="secondary" onClick={() => { setSource("pdf"); setNav({ level: "subjects" }); }}>PDF archive</button>
          </div>
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

