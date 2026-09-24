export function SixtyNineFold({ active }: { active: boolean }) {
  return (
    <div className={`sixty-nine ${active ? 'folding' : ''}`} aria-hidden="true">
      <span className="sixty-nine-digit sixty-nine-six">6</span>
      <span className="sixty-nine-digit sixty-nine-nine">9</span>
    </div>
  )
}
