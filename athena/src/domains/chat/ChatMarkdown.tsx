import React from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

type Props = {
  content: string;
  className?: string;
  /** Optional find-in-chat highlight — applied inside text nodes when query set. */
  highlightQuery?: string;
};

function highlightText(text: string, query: string): React.ReactNode {
  const q = query.trim();
  if (!q) return text;
  const escaped = q.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const re = new RegExp(`(${escaped})`, "gi");
  const parts = text.split(re);
  const test = new RegExp(`^${escaped}$`, "i");
  return parts.map((p, i) =>
    test.test(p) ? (
      <mark key={i} className="find-in-chat-highlight">
        {p}
      </mark>
    ) : (
      <React.Fragment key={i}>{p}</React.Fragment>
    ),
  );
}

/** Full markdown + GFM for chat assistant/user bubbles. */
export function ChatMarkdown({ content, className, highlightQuery = "" }: Props) {
  return (
    <div className={`chat-md${className ? ` ${className}` : ""}`}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          code({ className: cn, children, ...props }) {
            const text = String(children).replace(/\n$/, "");
            const isBlock = Boolean(cn) || text.includes("\n");
            if (!isBlock) {
              return (
                <code className="message-inline-code" {...props}>
                  {highlightQuery ? highlightText(text, highlightQuery) : text}
                </code>
              );
            }
            const lang = /language-(\w+)/.exec(cn || "")?.[1];
            return (
              <div className="chat-md-codeblock">
                {lang ? <div className="chat-md-codeblock-lang">{lang}</div> : null}
                <pre className="chat-md-pre">
                  <code className={cn} {...props}>
                    {text}
                  </code>
                </pre>
              </div>
            );
          },
          a({ href, children }) {
            return (
              <a href={href} target="_blank" rel="noreferrer noopener">
                {children}
              </a>
            );
          },
          p({ children }) {
            return <p className="chat-md-p">{children}</p>;
          },
          ul({ children }) {
            return <ul className="chat-md-list">{children}</ul>;
          },
          ol({ children }) {
            return <ol className="chat-md-list">{children}</ol>;
          },
          blockquote({ children }) {
            return <blockquote className="chat-md-quote">{children}</blockquote>;
          },
          table({ children }) {
            return (
              <div className="chat-md-table-wrap">
                <table>{children}</table>
              </div>
            );
          },
        }}
      >
        {content}
      </ReactMarkdown>
    </div>
  );
}
