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
  // left side, curling up and out
  { x: '40px', y: '150px', w: 90, h: 70, delay: '0s', drift: '-54px', rise: '-70px', sway: '-16px' },
  { x: '18px', y: '188px', w: 72, h: 96, delay: '0.1s', drift: '-46px', rise: '-100px', sway: '-12px' },
  { x: '64px', y: '210px', w: 64, h: 80, delay: '0.38s', drift: '-50px', rise: '-120px', sway: '-10px' },
  { x: '8px', y: '164px', w: 110, h: 78, delay: '0.2s', drift: '-62px', rise: '-60px', sway: '-20px' },
  // right side
  { x: 'calc(100% - 40px)', y: '146px', w: 96, h: 74, delay: '0.04s', drift: '56px', rise: '-74px', sway: '14px' },
  { x: 'calc(100% - 16px)', y: '190px', w: 70, h: 100, delay: '0.14s', drift: '48px', rise: '-108px', sway: '16px' },
  { x: 'calc(100% - 62px)', y: '214px', w: 60, h: 82, delay: '0.44s', drift: '52px', rise: '-116px', sway: '10px' },
  { x: 'calc(100% - 6px)', y: '168px', w: 112, h: 76, delay: '0.24s', drift: '64px', rise: '-64px', sway: '18px' },
  // above the window
  { x: '30%', y: '128px', w: 130, h: 80, delay: '0s', drift: '-22px', rise: '-90px', sway: '12px' },
  { x: '50%', y: '136px', w: 150, h: 90, delay: '0.08s', drift: '6px', rise: '-104px', sway: '-14px' },
  { x: '68%', y: '124px', w: 140, h: 84, delay: '0.05s', drift: '26px', rise: '-86px', sway: '10px' },
  { x: '40%', y: '108px', w: 80, h: 100, delay: '0.32s', drift: '-12px', rise: '-110px', sway: '8px' },
  { x: '60%', y: '112px', w: 76, h: 96, delay: '0.36s', drift: '16px', rise: '-112px', sway: '-8px' },
  { x: '22%', y: '140px', w: 100, h: 70, delay: '0.16s', drift: '-36px', rise: '-80px', sway: '-14px' },
  { x: '78%', y: '136px', w: 100, h: 68, delay: '0.2s', drift: '38px', rise: '-84px', sway: '12px' },
  { x: '48%', y: '96px', w: 170, h: 100, delay: '0.14s', drift: '2px', rise: '-70px', sway: '-6px' },
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
