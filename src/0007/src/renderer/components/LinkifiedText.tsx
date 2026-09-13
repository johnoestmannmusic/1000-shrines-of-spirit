import { Fragment, type ReactNode } from "react";
import { openExternal } from "../platform";

const URL_SPLIT = /(https?:\/\/[^\s]+)/g;
const URL_TEST = /^https?:\/\//;
const TRAILING = /[.,;:!?)\]}'"]+$/;

/**
 * Renders text with any `https://` word turned into a link that opens in the
 * system browser (desktop) or a new tab (web). Pair with the `.linkified`
 * class so long unbroken words/URLs also wrap.
 */
export function LinkifiedText({ text }: { text: string }) {
  if (!text) return <>{text}</>;
  const nodes: ReactNode[] = [];
  let last = 0;
  URL_SPLIT.lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = URL_SPLIT.exec(text)) !== null) {
    if (match.index > last) nodes.push(text.slice(last, match.index));
    let url = match[1]!;
    const trailing = url.match(TRAILING)?.[0] ?? "";
    if (trailing) url = url.slice(0, url.length - trailing.length);
    if (URL_TEST.test(url)) {
      nodes.push(
        <a
          key={`${match.index}-link`}
          href={url}
          onClick={(event) => {
            event.preventDefault();
            void openExternal(url);
          }}
        >
          {url}
        </a>,
      );
      if (trailing) nodes.push(<Fragment key={`${match.index}-tail`}>{trailing}</Fragment>);
    } else {
      nodes.push(match[0]);
    }
    last = match.index + match[0].length;
  }
  if (last < text.length) nodes.push(text.slice(last));
  return <>{nodes}</>;
}
