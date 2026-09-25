import { nativeWindow } from '../lib/bridge'
import { wideSmokeRoom } from '../lib/smokeRoom'

// placed against the bar: `at` is its left edge, its right edge, or a fraction along its top; y is below the bar top.
// the layer sets --bar-x and --bar-top for the room it has
type Puff = {
  at: 'left' | 'right' | number
  x: number
  y: number
  w: number
  h: number
  delay: string
  drift: string
  rise: string
  sway: string
}

const PUFFS: Puff[] = [
  // off the top, overlapping into one cloud that fans out as it rises
  { at: 0.08, x: 0, y: 10, w: 150, h: 100, delay: '0.04s', drift: '-58px', rise: '-70px', sway: '-30px' },
  { at: 0.2, x: 0, y: -4, w: 190, h: 110, delay: '0s', drift: '-44px', rise: '-92px', sway: '-16px' },
  { at: 0.33, x: 0, y: 8, w: 170, h: 104, delay: '0.1s', drift: '-30px', rise: '-108px', sway: '8px' },
  { at: 0.47, x: 0, y: -10, w: 220, h: 120, delay: '0.02s', drift: '-8px', rise: '-96px', sway: '-6px' },
  { at: 0.6, x: 0, y: 6, w: 180, h: 110, delay: '0.07s', drift: '18px', rise: '-112px', sway: '10px' },
  { at: 0.72, x: 0, y: -6, w: 200, h: 112, delay: '0.12s', drift: '36px', rise: '-94px', sway: '-8px' },
  { at: 0.86, x: 0, y: 10, w: 160, h: 100, delay: '0.03s', drift: '52px', rise: '-80px', sway: '22px' },
  { at: 0.15, x: 0, y: -20, w: 110, h: 110, delay: '0.44s', drift: '-48px', rise: '-74px', sway: '-20px' },
  { at: 0.4, x: 0, y: -30, w: 130, h: 130, delay: '0.3s', drift: '-20px', rise: '-84px', sway: '-10px' },
  { at: 0.52, x: 0, y: -48, w: 240, h: 110, delay: '0.2s', drift: '4px', rise: '-60px', sway: '6px' },
  { at: 0.62, x: 0, y: -26, w: 120, h: 124, delay: '0.36s', drift: '24px', rise: '-88px', sway: '8px' },
  { at: 0.83, x: 0, y: -22, w: 110, h: 110, delay: '0.48s', drift: '46px', rise: '-76px', sway: '20px' },
  // the corners, out and up
  { at: 'left', x: 16, y: 4, w: 140, h: 100, delay: '0.05s', drift: '-74px', rise: '-76px', sway: '-44px' },
  { at: 'right', x: -16, y: 4, w: 140, h: 100, delay: '0.08s', drift: '74px', rise: '-76px', sway: '44px' },
  // beside the bar, rolling out and curling up
  { at: 'left', x: -4, y: 28, w: 110, h: 96, delay: '0s', drift: '-70px', rise: '-56px', sway: '-46px' },
  { at: 'left', x: 6, y: 54, w: 96, h: 104, delay: '0.12s', drift: '-62px', rise: '-80px', sway: '-40px' },
  { at: 'left', x: -10, y: 40, w: 120, h: 100, delay: '0.3s', drift: '-78px', rise: '-44px', sway: '-54px' },
  { at: 'left', x: 10, y: 70, w: 80, h: 80, delay: '0.42s', drift: '-52px', rise: '-64px', sway: '-36px' },
  { at: 'right', x: 4, y: 30, w: 112, h: 94, delay: '0.04s', drift: '72px', rise: '-58px', sway: '48px' },
  { at: 'right', x: -6, y: 56, w: 94, h: 106, delay: '0.16s', drift: '60px', rise: '-82px', sway: '40px' },
  { at: 'right', x: 10, y: 42, w: 118, h: 98, delay: '0.26s', drift: '80px', rise: '-46px', sway: '56px' },
  { at: 'right', x: -10, y: 68, w: 82, h: 82, delay: '0.46s', drift: '54px', rise: '-62px', sway: '34px' },
]

function puffLeft({ at, x }: Puff): string {
  if (at === 'left') return `calc(var(--bar-x) + ${x}px)`
  if (at === 'right') return `calc(100% - var(--bar-x) + ${x}px)`
  return `calc(var(--bar-x) + (100% - 2 * var(--bar-x)) * ${at} + ${x}px)`
}

export function FourTwentySmoke({ smoking }: { smoking: boolean }) {
  const state = smoking ? (wideSmokeRoom(nativeWindow()) ? 'smoking wide' : 'smoking') : ''
  return (
    <>
      <div className={`four-twenty-room ${state}`} />
      <div className={`four-twenty ${state}`} aria-hidden="true">
        {PUFFS.map((puff, i) => (
          <span
            key={i}
            className="four-twenty-puff"
            style={{
              left: puffLeft(puff),
              top: `calc(var(--bar-top) + ${puff.y}px)`,
              width: puff.w,
              height: puff.h,
              animationDelay: puff.delay,
              ['--drift' as string]: puff.drift,
              ['--rise' as string]: puff.rise,
              ['--sway' as string]: puff.sway,
            }}
          />
        ))}
      </div>
    </>
  )
}
