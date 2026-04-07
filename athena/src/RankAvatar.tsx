import auxiliaryUserUrl from "./assets/ranks/auxiliary-user.png";
import caesarLaurelUrl from "./assets/ranks/caesar-laurel.png";
import centurionCoinUrl from "./assets/ranks/centurion-coin.png";
import legionaryHelmetUrl from "./assets/ranks/legionary-helmet.png";
import generalCoinUrl from "./assets/ranks/general-coin.png";
import { RANK_DESCRIPTIONS, clampRankLevel } from "./romanRanks";

type RankAvatarProps = {
  rankLevel: number;
  /** Nav card (compact) or Me detail header (large). */
  variant: "nav" | "hero";
  className?: string;
};

/**
 * Rank glyph per level — each level uses supplied artwork (mono line art or full color).
 */
export function RankAvatar({ rankLevel, variant, className = "" }: RankAvatarProps) {
  const level = clampRankLevel(rankLevel);
  const title = RANK_DESCRIPTIONS[level];

  return (
    <span
      className={`rank-avatar rank-avatar--level-${level} rank-avatar--${variant} ${className}`.trim()}
      title={title}
      aria-hidden
    >
      {level === 1 ? (
        <img
          src={auxiliaryUserUrl}
          alt=""
          className="rank-avatar__glyph-img rank-avatar__glyph-img--mono"
          draggable={false}
        />
      ) : level === 2 ? (
        <img
          src={legionaryHelmetUrl}
          alt=""
          className="rank-avatar__glyph-img rank-avatar__glyph-img--mono"
          draggable={false}
        />
      ) : level === 3 ? (
        <img
          src={centurionCoinUrl}
          alt=""
          className="rank-avatar__glyph-img rank-avatar__glyph-img--mono"
          draggable={false}
        />
      ) : level === 4 ? (
        <img
          src={generalCoinUrl}
          alt=""
          className="rank-avatar__glyph-img rank-avatar__glyph-img--color"
          draggable={false}
        />
      ) : (
        <img
          src={caesarLaurelUrl}
          alt=""
          className="rank-avatar__caesar-img"
          draggable={false}
        />
      )}
    </span>
  );
}
