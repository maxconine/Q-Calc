export function SixtySevenArms({ shaking }: { shaking: boolean }) {
  return (
    <div className={`sixty-seven-arms ${shaking ? 'shaking' : ''}`} aria-hidden="true">
      {(['left', 'right'] as const).map((side) => (
        <svg key={side} className={`sixty-seven-arm sixty-seven-arm-${side}`} viewBox="0 0 44 30">
          {/* a relaxed palm-up hand in profile: heel, four fingers cupping upward, thumb angled forward */}
          <g
            fill="currentColor"
            stroke="currentColor"
            strokeLinecap="round"
            transform={side === 'right' ? 'matrix(-1 0 0 1 44 0)' : undefined}
          >
            <path d="M4 20.8C4 16.2 7.4 13.6 12 13.5L20.2 13.8C23 14 24.2 16.4 24.2 19.8C24.2 24.3 21 26.9 15.6 26.9H11C6.8 26.9 4 24.6 4 20.8Z" stroke="none" />
            <g fill="none">
              <path d="M20 15Q29 15.2 35.6 11.2" strokeWidth="3.4" />
              <path d="M21 18.4Q31 19.2 38.8 15.9" strokeWidth="3.5" />
              <path d="M21 21.9Q31 23 38.4 20.6" strokeWidth="3.3" />
              <path d="M20 25.2Q28 26.5 34.2 25" strokeWidth="3" />
              <path d="M10.5 14.5Q12.6 8.6 18 6" strokeWidth="4" />
            </g>
          </g>
        </svg>
      ))}
    </div>
  )
}
