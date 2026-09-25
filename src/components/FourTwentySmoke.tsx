// anchors are inside a layer that starts 128px above the card and hangs 56px past each side
const PUFFS: Array<{
  x: string
  y: string
  w: number
  h: number
  delay: string
  drift: string
  rise: string
  sway: string
}> = [
  // left side, rolling out and curling
  { x: '40px', y: '150px', w: 90, h: 70, delay: '0s', drift: '-97px', rise: '-42px', sway: '13px' },
  { x: '18px', y: '188px', w: 72, h: 96, delay: '0.1s', drift: '-83px', rise: '-60px', sway: '10px' },
  { x: '64px', y: '210px', w: 64, h: 80, delay: '0.38s', drift: '-90px', rise: '-72px', sway: '8px' },
  { x: '8px', y: '164px', w: 110, h: 78, delay: '0.2s', drift: '-112px', rise: '-36px', sway: '16px' },
  // right side
  { x: 'calc(100% - 40px)', y: '146px', w: 96, h: 74, delay: '0.04s', drift: '101px', rise: '-44px', sway: '-11px' },
  { x: 'calc(100% - 16px)', y: '190px', w: 70, h: 100, delay: '0.14s', drift: '86px', rise: '-65px', sway: '-13px' },
  { x: 'calc(100% - 62px)', y: '214px', w: 60, h: 82, delay: '0.44s', drift: '94px', rise: '-70px', sway: '-8px' },
  { x: 'calc(100% - 6px)', y: '168px', w: 112, h: 76, delay: '0.24s', drift: '115px', rise: '-38px', sway: '-14px' },
  // above the window
  { x: '30%', y: '128px', w: 130, h: 80, delay: '0s', drift: '-54px', rise: '-54px', sway: '-10px' },
  { x: '50%', y: '136px', w: 150, h: 90, delay: '0.08s', drift: '7px', rise: '-62px', sway: '11px' },
  { x: '68%', y: '124px', w: 140, h: 84, delay: '0.05s', drift: '56px', rise: '-52px', sway: '-8px' },
  { x: '40%', y: '108px', w: 80, h: 100, delay: '0.32s', drift: '-28px', rise: '-66px', sway: '-6px' },
  { x: '60%', y: '112px', w: 76, h: 96, delay: '0.36s', drift: '33px', rise: '-67px', sway: '6px' },
  { x: '22%', y: '140px', w: 100, h: 70, delay: '0.16s', drift: '-82px', rise: '-48px', sway: '11px' },
  { x: '78%', y: '136px', w: 100, h: 68, delay: '0.2s', drift: '85px', rise: '-50px', sway: '-10px' },
  { x: '48%', y: '96px', w: 170, h: 100, delay: '0.14s', drift: '0px', rise: '-42px', sway: '5px' },
]

export function FourTwentySmoke({ smoking }: { smoking: boolean }) {
  return (
    <>
      <div className={`four-twenty-room ${smoking ? 'smoking' : ''}`} />
      <div className={`four-twenty ${smoking ? 'smoking' : ''}`} aria-hidden="true">
        {PUFFS.map((puff, i) => (
          <span
            key={i}
            className="four-twenty-puff"
            style={{
              left: puff.x,
              top: puff.y,
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
