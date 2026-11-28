import json, re, math, sys, unicodedata
def plain(s):
    # raised and lowered text as it would be typed: e^{−at} → e^(−at), a_{k} → a_k
    prev=None
    while prev!=s:
        prev=s
        # a space after a lowered run a letter follows: ω_{o}t copies as ω_o t
        s=re.sub(r'_\{([^{}]*)\}(?=[^\W_])', r'_\1 ', s)
        s=re.sub(r'_\{([^{}]*)\}', r'_\1', s)
        s=re.sub(r'\^\{([^{}]*)\}', lambda m: '^'+m.group(1) if re.fullmatch(r'[\w−-]{1}|\d+',m.group(1)) else '^('+m.group(1)+')', s)
    return s.replace('−','-')
COND=re.compile(r',\s*(?:a > 0|a ≥ 0|\|a\| < 1|0 < W < π)$')
def bare(s): return plain(COND.sub('',s))
def I(show, note=None, lead=None, copy=None):
    d={}
    if lead: d['lead']=lead
    d['show']=show
    # a lead here is a label (CT, Analysis): never copied
    d['copy']=copy if copy is not None else plain(show)
    if note: d['note']=note
    return d
def P(a,b,note=None):
    "a transform table row: the two sides in columns"
    # each side copies itself, without its condition
    return {'lead':[a],'leadCopy':[bare(a)],'show':b,'copy':bare(b),**({'note':note} if note else {})}
def H(*cells):
    "a table's heading row: not copied"
    return {'lead':list(cells[:-1]),'show':cells[-1],'copy':''}
def C(*cells,note=None):
    "a row of a plain table"
    # each cell copies itself
    return {'lead':list(cells[:-1]),'leadCopy':[plain(c) for c in cells[:-1]],'show':cells[-1],'copy':plain(cells[-1]),**({'note':note} if note else {})}
def L(*cells,note=None):
    "a row whose first cell is its name, which isn't copied"
    d=C(*cells,note=note); d['leadCopy'][0]=''; return d
def G(title,*items): return {'title':title,'items':list(items)}
def S(title,*groups,wide=False,leads=None):
    d={'title':title}
    if wide: d['wide']=True
    if leads: d['leadWidths']=leads
    d['groups']=list(groups)
    return d
def SH(id,title,hint,*sections): return {'id':id,'title':title,'hint':hint,'sections':list(sections)}

w0='ω_{o}'
fourier=SH('fourier','Fourier Series','fourier series',
 S('Exponential form',
  G('Analysis: the coefficients a_k',
    I('a_{k} = (1/T) ∫_{T} x(t) e^{−jkω_{o}t} dt',note='ωₒ = 2π/T',lead=['CT']),
    I('a_{k} = (1/N) Σ_{n=<N>} x[n] e^{−jkω_{o}n}',note='ωₒ = 2π/N',lead=['DT']),
    I('b_{k} = a_{k} H(jkω_{o})',note='output',lead=['LTI'])),
  G('Synthesis: the series',
    I('x(t) = Σ_{k=−∞}^{∞} a_{k} e^{jkω_{o}t}',note='ωₒ = 2π/T',lead=['CT']),
    I('x[n] = Σ_{k=<N>} a_{k} e^{jkω_{o}n}',note='N terms',lead=['DT'])),
  leads=[44]),
 S('Trig form (real x)',
  G('Series: sine-cosine, then amplitude-phase',
    I('x(t) = A_{0} + Σ_{k=1}^{∞} [A_{k} cos(kω_{o}t) + B_{k} sin(kω_{o}t)]'),
    I('x(t) = A_{0} + Σ_{k=1}^{∞} C_{k} cos(kω_{o}t + φ_{k})')),
  G('Coefficients',
    I('A_{0} = (1/T) ∫_{T} x(t) dt',note='DC term: the average'),
    I('A_{k} = (2/T) ∫_{T} x(t) cos(kω_{o}t) dt',note='cosine terms'),
    I('B_{k} = (2/T) ∫_{T} x(t) sin(kω_{o}t) dt',note='sine terms'),
    I('C_{k} = √(A_{k}^{2} + B_{k}^{2}) = 2|a_{k}|',note='harmonic amplitude'),
    I('φ_{k} = −atan2(B_{k}, A_{k})',note='harmonic phase')),
  G('Linking the two forms',
    I('a_{k} = (A_{k} − jB_{k})/2',note='k ≥ 1, and a₀ = A₀'),
    I('a_{−k} = a_{k}^{*}',note='x real: conjugate symmetry')),
  G('Symmetry shortcuts',
    I('B_{k} = 0',note='x even: cosine terms only'),
    I('A_{0} = A_{k} = 0',note='x odd: sine terms only'))),
 S('Common series',
  G('Standard waveforms, amplitude A, ωₒ = 2π/T',
    I('x(t) = (4A/π) Σ_{k=1,3,5,…} sin(kω_{o}t)/k',note='square wave ±A, odd: odd harmonics only'),
    I('a_{k} = sin(kω_{o}T_{1})/(kπ)',note='pulse train, 1 for |t| < T₁: a₀ = 2T₁/T'),
    I('x(t) = (8A/π^{2}) Σ_{k=1,3,5,…} cos(kω_{o}t)/k^{2}',note='triangle wave ±A, even: odd harmonics, 1/k²'),
    I('x(t) = (2A/π) Σ_{k=1}^{∞} (−1)^{k+1} sin(kω_{o}t)/k',note='sawtooth −A to A, odd'),
    I('a_{k} = 1/T',note='impulse train Σ δ(t − nT): every harmonic')),
  wide=True),
 S('Properties (CT)',
  G('x(t) has coefficients a_k, y(t) has b_k, both period T',
    H('Signal','Coefficient'),
    C('A x(t) + B y(t)','A a_{k} + B b_{k}',note='linearity'),
    C('x(t − t_{0})','a_{k} e^{−jkω_{o}t_{0}}',note='time shift'),
    C('e^{jMω_{o}t} x(t)','a_{k−M}',note='frequency shift'),
    C('x(−t)','a_{−k}',note='time reversal'),
    C('x^{*}(t)','a_{−k}^{*}',note='conjugation'),
    C('dx/dt','jkω_{o} a_{k}',note='differentiation'),
    C('∫_{−∞}^{t} x(τ) dτ','a_{k}/(jkω_{o})',note='integration, needs a₀ = 0'),
    C('∫_{T} x(τ) y(t − τ) dτ','T a_{k} b_{k}',note='periodic convolution'),
    C('x(t) y(t)','Σ_{l} a_{l} b_{k−l}',note='multiplication'),
    C('(1/T) ∫_{T} |x(t)|^{2} dt','Σ_{k} |a_{k}|^{2}',note='Parseval: average power')),
  wide=True,leads=[180]))

euler=SH('euler',"Euler's Formulas","euler's formulas and related identities",
 S("Euler's formulas",
  G('Exponentials',
    I('e^{jθ} = cos(θ) + j sin(θ)',note="Euler's formula"),
    I('e^{jπ} + 1 = 0',note="Euler's identity"),
    I('e^{jθ} + e^{−jθ} = 2 cos(θ)'),
    I('e^{jθ} − e^{−jθ} = 2j sin(θ)')),
  G('Trig from exponentials',
    I('cos(θ) = (1/2) e^{jθ} + (1/2) e^{−jθ}'),
    I('sin(θ) = (1/2j) e^{jθ} − (1/2j) e^{−jθ}'),
    I('(cos θ + j sin θ)^{n} = cos(nθ) + j sin(nθ)',note='de Moivre')),
  G('Hyperbolic',
    I('cosh(x) = (e^{x} + e^{−x})/2'),
    I('sinh(x) = (e^{x} − e^{−x})/2'),
    I('cos(jx) = cosh(x)'),
    I('sin(jx) = j sinh(x)'),
    I('cosh^{2}(x) − sinh^{2}(x) = 1',copy='cosh(x)^2 - sinh(x)^2 = 1'))),
 S('Useful trig identities',
  G('Angle addition and double angle',
    I('cos(A ± B) = cos(A) cos(B) ∓ sin(A) sin(B)'),
    I('sin(A ± B) = sin(A) cos(B) ± cos(A) sin(B)'),
    I('sin(2A) = 2 sin(A) cos(A)',note='double angle'),
    I('cos(2A) = cos^{2}(A) − sin^{2}(A)',note='double angle',copy='cos(2A) = cos(A)^2 - sin(A)^2'),
    I('cos(2A) = 2 cos^{2}(A) − 1 = 1 − 2 sin^{2}(A)',note='double angle, other forms',copy='cos(2A) = 2 cos(A)^2 - 1')),
  G('Sum of sinusoids, R = √(a² + b²), φ = atan2(b, a)',
    I('a cos(θ) + b sin(θ) = R cos(θ − φ)',note='harmonic addition'),
    I('cos(A) + cos(B) = 2 cos((A + B)/2) cos((A − B)/2)',note='sum to product, beats')),
  G('Squares',
    I('sin^{2}(A) + cos^{2}(A) = 1',note='Pythagorean',copy='sin(A)^2 + cos(A)^2 = 1'),
    I('cos^{2}(A) = 1/2 + (1/2) cos(2A)',note='power reduction',copy='cos(A)^2 = 1/2 + (1/2) cos(2A)'),
    I('sin^{2}(A) = 1/2 − (1/2) cos(2A)',note='power reduction',copy='sin(A)^2 = 1/2 - (1/2) cos(2A)')),
  G('Products to sums',
    I('cos(A) cos(B) = (1/2)[cos(A − B) + cos(A + B)]'),
    I('sin(A) sin(B) = (1/2)[cos(A − B) − cos(A + B)]'),
    I('sin(A) cos(B) = (1/2)[sin(A + B) + sin(A − B)]'),
    I('cos(A) sin(B) = (1/2)[sin(A + B) − sin(A − B)]')),
  wide=True))

complexn=SH('complex numbers','Complex Numbers','complex numbers: rectangular and polar, arithmetic, conjugate, roots, special values',
 S('Rectangular and polar',
  G('Converting',
    I('z = x + jy = r e^{jθ}',note='rectangular = polar'),
    I('e^{jθ} = cos(θ) + j sin(θ)',note="Euler's formula"),
    I('r = |z| = √(x^{2} + y^{2})',note='magnitude, modulus'),
    I('θ = atan2(y, x)',note='angle, argument'),
    I('x = r cos(θ)',note='real part'),
    I('y = r sin(θ)',note='imaginary part')),
  G('Arithmetic',
    I('(a + jb)(c + jd) = (ac − bd) + j(ad + bc)',note='multiply'),
    I('(a + jb)/(c + jd) = ((ac + bd) + j(bc − ad))/(c^{2} + d^{2})',note='divide'),
    I('z_{1} z_{2} = r_{1} r_{2} e^{j(θ_{1} + θ_{2})}',note='magnitudes multiply, angles add'),
    I('z_{1} / z_{2} = (r_{1}/r_{2}) e^{j(θ_{1} − θ_{2})}',note='magnitudes divide, angles subtract')),
  G('Conjugate and modulus',
    I('z^{*} = x − jy = r e^{−jθ}',note='complex conjugate'),
    I('z z^{*} = |z|^{2}',note='modulus squared'),
    I('Re z = (z + z^{*})/2',note='real part'),
    I('Im z = (z − z^{*})/2j',note='imaginary part'),
    I('|z_{1} + z_{2}| ≤ |z_{1}| + |z_{2}|',note='triangle inequality')),
  G('Powers, roots and logs',
    I('z^{n} = r^{n} e^{jnθ}',note='de Moivre'),
    I('z^{1/n} = r^{1/n} e^{j(θ + 2πk)/n}',note='nth roots, k = 0, 1, …, n − 1'),
    I('ln z = ln r + j(θ + 2πk)',note='complex log, k = 0 principal'))),
 S('Special values in polar form',
  G('Common angles',
    I('e^{j0} = 1'),
    I('e^{±j2π} = 1'),
    I('e^{±jπ} = −1'),
    I('e^{jπ/2} = j'),
    I('e^{−jπ/2} = −j'),
    I('e^{jπ/4} = (1 + j)/√2'),
    I('e^{jπ/3} = (1 + j√3)/2'),
    I('e^{jπ/6} = (√3 + j)/2')),
  G('With a variable',
    I('e^{j0t} = 1',note='t real'),
    I('e^{jπk} = e^{−jπk} = (−1)^{k}',note='k an integer'),
    I('e^{±j2πk} = 1',note='k an integer')),
  G('The imaginary unit',
    I('j^{2} = −1'),
    I('j^{3} = −j,  j^{4} = 1',note='powers of j repeat every 4',copy='j^4 = 1'),
    I('1/j = −j'),
    I('j = i',note='either works in Q Calc'))))

ct=SH('ct transform table','CT Fourier Transform Table','ct fourier transform table',
 S('Definition',
  G('The pair',
    I('X(jω) = ∫_{−∞}^{∞} x(t) e^{−jωt} dt',lead=['Analysis']),
    I('x(t) = (1/2π) ∫_{−∞}^{∞} X(jω) e^{jωt} dω',lead=['Synthesis'])),
  G('LTI systems',
    I('H(jω) = Y(jω)/X(jω) = (Σ b_{k}(jω)^{k})/(Σ a_{k}(jω)^{k})',lead=['Response'],note='from Σ aₖ dᵏy/dtᵏ = Σ bₖ dᵏx/dtᵏ'),
    I('y(t) = |H(jω_{o})| cos(ω_{o}t + ∠H(jω_{o}))',lead=['Sinusoid'],note='steady output for the input cos(ωₒt)')),
  wide=True,leads=[80]),
 S('Pairs',
  G('Common signals',
    H('Time','Frequency'),
    P('δ(t)','1'),
    P('δ(t − t_{0})','e^{−jωt_{0}}'),
    P('1','2πδ(ω)'),
    P('u(t)','1/(jω) + πδ(ω)'),
    P('sgn(t)','2/(jω)',note='sign function'),
    P('e^{−at} u(t), a > 0','1/(a + jω)'),
    P('t e^{−at} u(t), a > 0','1/(a + jω)^{2}'),
    P('t^{n−1} e^{−at} u(t)/(n − 1)!, a > 0','1/(a + jω)^{n}'),
    P('e^{−a|t|}, a > 0','2a/(a^{2} + ω^{2})'),
    P('e^{−at} cos(ω_{o}t) u(t), a > 0','(a + jω)/((a + jω)^{2} + ω_{o}^{2})',note='damped cosine'),
    P('e^{−at} sin(ω_{o}t) u(t), a > 0','ω_{o}/((a + jω)^{2} + ω_{o}^{2})',note='damped sine'),
    P('e^{jω_{o}t}','2πδ(ω − ω_{o})'),
    P('cos(ω_{o}t)','πδ(ω − ω_{o}) + πδ(ω + ω_{o})'),
    P('sin(ω_{o}t)','(π/j)[δ(ω − ω_{o}) − δ(ω + ω_{o})]'),
    P('1 for |t| < T_{1}, 0 else','2 sin(ωT_{1})/ω',note='rect pulse to sinc'),
    P('sin(Wt)/(πt)','1 for |ω| < W, 0 else',note='sinc to ideal low-pass'),
    P('1 − |t|/T for |t| < T, 0 else','T sin^{2}(ωT/2)/(ωT/2)^{2}',note='triangle pulse'),
    P('e^{−t^{2}/(2σ^{2})}','σ√(2π) e^{−σ^{2}ω^{2}/2}',note='Gaussian'),
    P('Periodic p(t) with F.S. {ω_{o}, a_{k}}','P(jω) = Σ_{k=−∞}^{∞} 2πa_{k} δ(ω − kω_{o})'),
    P('Σ_{n=−∞}^{∞} δ(t − nT)','(2π/T) Σ_{k=−∞}^{∞} δ(ω − 2πk/T)',note='impulse train')),
  wide=True,leads=[270]),
 S('Properties',
  G('If x(t) has transform X(jω)',
    H('Time','Frequency'),
    P('a x(t) + b y(t)','a X(jω) + b Y(jω)',note='linearity'),
    P('x(t − t_{0})','e^{−jωt_{0}} X(jω)',note='time shift'),
    P('e^{jω_{o}t} x(t)','X(j(ω − ω_{o}))',note='frequency shift'),
    P('x(t) cos(ω_{o}t)','(1/2)[X(j(ω − ω_{o})) + X(j(ω + ω_{o}))]',note='modulation, AM'),
    P('x(at)','(1/|a|) X(jω/a)',note='time scaling'),
    P('x(−t)','X(−jω)',note='time reversal'),
    P('x^{*}(t)','X^{*}(−jω)',note='conjugation'),
    P('d^{n}x(t)/dt^{n}','(jω)^{n} X(jω)',note='differentiation'),
    P('∫_{−∞}^{t} x(τ) dτ','X(jω)/(jω) + πX(0)δ(ω)',note='integration'),
    P('t x(t)','j dX(jω)/dω',note='frequency differentiation'),
    P('y(t) = x(t) ∗ h(t)','Y(jω) = X(jω) H(jω)',note='convolution'),
    P('y(t) = x(t) p(t)','Y(jω) = (1/2π) X(jω) ∗ P(jω)',note='multiplication, modulation'),
    P('x(t) Σ_{n} δ(t − nT)','(1/T) Σ_{k} X(j(ω − 2πk/T))',note='impulse sampling: copies every ωₛ = 2π/T'),
    P('X(jt)','2πx(−ω)',note='duality'),
    P('∫ |x(t)|^{2} dt','(1/2π) ∫ |X(jω)|^{2} dω',note='Parseval: energy')),
  wide=True,leads=[270]))

dt=SH('dt transform table','DT Fourier Transform Table','dt fourier transform table',
 S('Definition',
  G('The pair',
    I('X(e^{jω}) = Σ_{n=−∞}^{∞} x[n] e^{−jωn}',lead=['Analysis']),
    I('x[n] = (1/2π) ∫_{2π} X(e^{jω}) e^{jωn} dω',lead=['Synthesis'])),
  G('LTI systems and sampling',
    I('H(e^{jω}) = (Σ b_{k} e^{−jωk})/(Σ a_{k} e^{−jωk})',lead=['Response'],note='from Σ aₖ y[n − k] = Σ bₖ x[n − k]'),
    I('ω = ΩT = 2πf/f_{s}',lead=['Sampling'],note='DT frequency in rad/sample from Ω in rad/s')),
  wide=True,leads=[80]),
 S('Pairs',
  G('Common signals (every X repeats each 2π; impulses are shown for one period)',
    H('Sample','Frequency'),
    P('δ[n]','1'),
    P('δ[n − n_{o}]','e^{−jωn_{o}}'),
    P('1','2πδ(ω)'),
    P('u[n]','1/(1 − e^{−jω}) + πδ(ω)'),
    P('a^{n} u[n], |a| < 1','1/(1 − ae^{−jω})'),
    P('(n + 1) a^{n} u[n], |a| < 1','1/(1 − ae^{−jω})^{2}'),
    P('a^{|n|}, |a| < 1','(1 − a^{2})/(1 − 2a cos(ω) + a^{2})'),
    P('e^{jω_{o}n}','2πδ(ω − ω_{o})'),
    P('cos(ω_{o}n)','πδ(ω − ω_{o}) + πδ(ω + ω_{o})'),
    P('sin(ω_{o}n)','(π/j)[δ(ω − ω_{o}) − δ(ω + ω_{o})]'),
    P('1 for |n| ≤ N_{1}, 0 else','sin(ω(N_{1} + 1/2))/sin(ω/2)',note='rect pulse'),
    P('1 for 0 ≤ n ≤ N − 1, 0 else','e^{−jω(N−1)/2} sin(ωN/2)/sin(ω/2)',note='causal rectangular window, length N'),
    P('sin(Wn)/(πn), 0 < W < π','1 for |ω| < W, 0 else',note='sinc to ideal low-pass'),
    P('Σ_{k=−∞}^{∞} δ[n − kN]','(2π/N) Σ_{k=−∞}^{∞} δ(ω − 2πk/N)',note='impulse train')),
  wide=True,leads=[270]),
 S('Properties',
  G('If x[n] has transform X(e^jω)',
    H('Sample','Frequency'),
    P('a x[n] + b y[n]','a X(e^{jω}) + b Y(e^{jω})',note='linearity'),
    P('x[n − n_{o}]','e^{−jωn_{o}} X(e^{jω})',note='time shift'),
    P('e^{jω_{o}n} x[n]','X(e^{j(ω − ω_{o})})',note='frequency shift'),
    P('x[n] cos(ω_{o}n)','(1/2)[X(e^{j(ω − ω_{o})}) + X(e^{j(ω + ω_{o})})]',note='modulation, windowed cosine'),
    P('x[−n]','X(e^{−jω})',note='time reversal'),
    P('x[n/k] if k divides n, 0 else','X(e^{jkω})',note='time expansion, upsampling'),
    P('x^{*}[n]','X^{*}(e^{−jω})',note='conjugation'),
    P('x[n] − x[n − 1]','(1 − e^{−jω}) X(e^{jω})',note='first difference'),
    P('Σ_{k=−∞}^{n} x[k]','X(e^{jω})/(1 − e^{−jω}) + πX(e^{j0}) δ(ω)',note='accumulation, one period'),
    P('n x[n]','j dX(e^{jω})/dω',note='frequency differentiation'),
    P('y[n] = x[n] ∗ h[n]','Y(e^{jω}) = X(e^{jω}) H(e^{jω})',note='convolution'),
    P('x[n] y[n]','(1/2π) ∫_{2π} X(e^{jθ}) Y(e^{j(ω − θ)}) dθ',note='multiplication'),
    P('Σ_{n} |x[n]|^{2}','(1/2π) ∫_{2π} |X(e^{jω})|^{2} dω',note='Parseval')),
  wide=True,leads=[270]),
 S('DFT',
  G('N-point',
    I('X[k] = Σ_{n=0}^{N−1} x[n] e^{−j2πkn/N}',lead=['Analysis']),
    I('x[n] = (1/N) Σ_{k=0}^{N−1} X[k] e^{j2πkn/N}',lead=['Synthesis']),
    I('f_{k} = k f_{s}/N',note='frequency of bin k; bins are f_s/N apart',lead=['Bins']),
    I('X[k] = X(e^{j2πk/N})',note='the DFT samples the DTFT; zero padding samples it finer',lead=['Samples'])),
  wide=True,leads=[80]))

laplace=SH('laplace','Laplace Transform Table','laplace transform table',
 S('Definition',
  G('The pair',
    I('F(s) = ∫_{0}^{∞} f(t) e^{−st} dt',note='one-sided transform'),
    I('f(t) = (1/2πj) ∫_{σ−j∞}^{σ+j∞} F(s) e^{st} ds',note='inverse: σ right of every pole')),wide=True),
 S('Pairs',
  G('For t ≥ 0, with the region of convergence',
    H('f(t)','F(s)'),
    P('δ(t)','1',note='all s'),
    P('δ(t − a), a ≥ 0','e^{−as}',note='all s'),
    P('u(t)','1/s',note='Re(s) > 0'),
    P('u(t − a), a ≥ 0','e^{−as}/s',note='Re(s) > 0'),
    P('t','1/s^{2}',note='Re(s) > 0'),
    P('t^{n}','n!/s^{n+1}',note='Re(s) > 0'),
    P('e^{−at}','1/(s + a)',note='Re(s) > −a'),
    P('1 − e^{−at}','a/(s(s + a))',note='Re(s) > max(0, −a)'),
    P('t e^{−at}','1/(s + a)^{2}',note='Re(s) > −a'),
    P('t^{n} e^{−at}','n!/(s + a)^{n+1}',note='Re(s) > −a'),
    P('(e^{−at} − e^{−bt})/(b − a)','1/((s + a)(s + b))',note='Re(s) > max(−a, −b)'),
    P('sin(ωt)','ω/(s^{2} + ω^{2})',note='Re(s) > 0'),
    P('cos(ωt)','s/(s^{2} + ω^{2})',note='Re(s) > 0'),
    P('t sin(ωt)','2ωs/(s^{2} + ω^{2})^{2}',note='Re(s) > 0'),
    P('t cos(ωt)','(s^{2} − ω^{2})/(s^{2} + ω^{2})^{2}',note='Re(s) > 0'),
    P('e^{−at} sin(ωt)','ω/((s + a)^{2} + ω^{2})',note='Re(s) > −a'),
    P('e^{−at} cos(ωt)','(s + a)/((s + a)^{2} + ω^{2})',note='Re(s) > −a'),
    P('sinh(at)','a/(s^{2} − a^{2})',note='Re(s) > |a|'),
    P('cosh(at)','s/(s^{2} − a^{2})',note='Re(s) > |a|')),
  wide=True,leads=[270]),
 S('Inverse by partial fractions',
  G('Split F(s), then invert each term from the pairs',
    I('F(s) = A_{1}/(s − p_{1}) + A_{2}/(s − p_{2}) + …',note='distinct poles: f(t) = Σ Aᵢ e^{pᵢt}'),
    I('A_{i} = [(s − p_{i}) F(s)]_{s = p_{i}}',note='cover-up rule: the residue at a simple pole'),
    I('A_{r−k} = (1/k!) [d^{k}/ds^{k} ((s − p)^{r} F(s))]_{s = p}',note='pole of order r: terms A_j/(s − p)^j'),
    I('s^{2} + bs + c = (s + b/2)^{2} + (c − b^{2}/4)',note='complex poles: complete the square, then the damped sine and cosine')),
  wide=True),
 S('Properties',
  G('If f(t) has transform F(s)',
    H('f(t)','F(s)'),
    P('a f(t) + b g(t)','a F(s) + b G(s)',note='linearity'),
    P('f(t − a) u(t − a), a ≥ 0','e^{−as} F(s)',note='time shift, delay'),
    P('g(t) u(t − a), a ≥ 0','e^{−as} ℒ{g(t + a)}',note='a piecewise f switched on at t = a'),
    P('e^{−at} f(t)','F(s + a)',note='frequency shift'),
    P('f(at), a > 0','(1/a) F(s/a)',note='time scaling'),
    P('df/dt','s F(s) − f(0)',note='derivative'),
    P('d^{2}f/dt^{2}','s^{2} F(s) − s f(0) − f′(0)',note='second derivative'),
    P('d^{n}f/dt^{n}','s^{n} F(s) − Σ_{k=1}^{n} s^{n−k} f^{(k−1)}(0)',note='nth derivative'),
    P('∫_{0}^{t} f(τ) dτ','F(s)/s',note='integral'),
    P('t f(t)','−dF/ds',note='multiply by t'),
    P('f(t) ∗ g(t)','F(s) G(s)',note='convolution')),
  G('Systems and periodic signals',
    I('H(s) = Y(s)/X(s)',note='transfer function, zero initial conditions; stable if every pole has Re(p) < 0'),
    I('F(s) = (1/(1 − e^{−sT})) ∫_{0}^{T} f(t) e^{−st} dt',note='f periodic with period T')),
  G('Limits',
    I('f(0⁺) = lim_{s→∞} s F(s)',note='initial value theorem'),
    I('f(∞) = lim_{s→0} s F(s)',note='final value theorem, if f settles: poles of sF(s) in the left half-plane')),
  wide=True,leads=[270]))

ztr=SH('z transform','Z Transform Table','z transform table',
 S('Definition',
  G('Two-sided',
    I('X(z) = Σ_{n=−∞}^{∞} x[n] z^{−n}'),
    I('z = e^{jω}',note='on the unit circle X(z) is the DTFT'),
    I('z = e^{sT}',note='s-plane to z-plane, sampling period T')),
  wide=True),
 S('Pairs',
  G('With the region of convergence',
    H('x[n]','X(z)'),
    P('δ[n]','1',note='all z'),
    P('δ[n − m]','z^{−m}',note='all z but 0, for m > 0'),
    P('u[n]','1/(1 − z^{−1})',note='|z| > 1'),
    P('n u[n]','z^{−1}/(1 − z^{−1})^{2}',note='|z| > 1'),
    P('a^{n} u[n]','1/(1 − az^{−1})',note='|z| > |a|'),
    P('−a^{n} u[−n − 1]','1/(1 − az^{−1})',note='|z| < |a|'),
    P('n a^{n} u[n]','az^{−1}/(1 − az^{−1})^{2}',note='|z| > |a|'),
    P('u[n] − u[n − N]','(1 − z^{−N})/(1 − z^{−1})',note='all z but 0: length-N window'),
    P('cos(ω_{o}n) u[n]','(1 − cos(ω_{o}) z^{−1})/(1 − 2cos(ω_{o}) z^{−1} + z^{−2})',note='|z| > 1'),
    P('sin(ω_{o}n) u[n]','sin(ω_{o}) z^{−1}/(1 − 2cos(ω_{o}) z^{−1} + z^{−2})',note='|z| > 1'),
    P('r^{n} cos(ω_{o}n) u[n]','(1 − r cos(ω_{o}) z^{−1})/(1 − 2r cos(ω_{o}) z^{−1} + r^{2} z^{−2})',note='|z| > r'),
    P('r^{n} sin(ω_{o}n) u[n]','r sin(ω_{o}) z^{−1}/(1 − 2r cos(ω_{o}) z^{−1} + r^{2} z^{−2})',note='|z| > r')),
  wide=True,leads=[190]),
 S('ROC, inversion and systems',
  G('Region of convergence, from the poles pᵢ',
    I('|z| > max|p_{i}|',note='right-sided (causal) x: outside the outermost pole'),
    I('|z| < min|p_{i}|',note='left-sided x: inside the innermost pole'),
    I('r_{1} < |z| < r_{2}',note='two-sided x: a ring between two poles')),
  G('Inverse by partial fractions, in powers of z^{−1}',
    I('X(z) = Σ A_{i}/(1 − p_{i} z^{−1})',note='x[n] = Σ Aᵢ pᵢⁿ u[n] when |z| > max|pᵢ|'),
    I('A_{i} = [(1 − p_{i} z^{−1}) X(z)]_{z = p_{i}}',note='cover-up rule: the residue at a simple pole')),
  G('Systems',
    I('H(z) = Y(z)/X(z) = (Σ b_{k} z^{−k})/(Σ a_{k} z^{−k})',note='from Σ aₖ y[n − k] = Σ bₖ x[n − k]'),
    I('|p_{i}| < 1 for every pole',note='causal and stable: the ROC holds the unit circle')),
  wide=True),
 S('Properties',
  G('If x[n] has transform X(z)',
    H('x[n]','X(z)'),
    P('a x[n] + b y[n]','a X(z) + b Y(z)',note='linearity'),
    P('x[n − n_{o}]','z^{−n_{o}} X(z)',note='time shift'),
    P('x[n − 1]','z^{−1} X(z) + x[−1]',note='one-sided: delay with an initial condition'),
    P('a^{n} x[n]','X(z/a)',note='scaling in z'),
    P('n x[n]','−z dX/dz',note='multiply by n'),
    P('x[−n]','X(1/z)',note='time reversal'),
    P('x[n] − x[n − 1]','(1 − z^{−1}) X(z)',note='first difference'),
    P('Σ_{k=−∞}^{n} x[k]','X(z)/(1 − z^{−1})',note='accumulation'),
    P('x[n] ∗ h[n]','X(z) H(z)',note='convolution')),
  G('Limits (x causal)',
    I('x[0] = lim_{z→∞} X(z)',note='initial value theorem'),
    I('x[∞] = lim_{z→1} (z − 1) X(z)',note='final value theorem, if x settles: poles of (z − 1)X(z) inside the unit circle')),
  wide=True,leads=[190]))

R3='√3'; R2='√2'
circle_rows=[(0,'0','0','1','0'),(30,'π/6','1/2',R3+'/2',R3+'/3'),(45,'π/4',R2+'/2',R2+'/2','1'),(60,'π/3',R3+'/2','1/2',R3),
 (90,'π/2','1','0','undefined'),(120,'2π/3',R3+'/2','−1/2','−'+R3),(135,'3π/4',R2+'/2','−'+R2+'/2','−1'),(150,'5π/6','1/2','−'+R3+'/2','−'+R3+'/3'),
 (180,'π','0','−1','0'),(210,'7π/6','−1/2','−'+R3+'/2',R3+'/3'),(225,'5π/4','−'+R2+'/2','−'+R2+'/2','1'),(240,'4π/3','−'+R3+'/2','−1/2',R3),
 (270,'3π/2','−1','0','undefined'),(300,'5π/3','−'+R3+'/2','1/2','−'+R3),(315,'7π/4','−'+R2+'/2',R2+'/2','−1'),(330,'11π/6','−1/2',R3+'/2','−'+R3+'/3'),(360,'2π','0','1','0')]
def val(s):
    return eval(s.replace('−','-').replace('√3','(3**.5)').replace('√2','(2**.5)'))
for d,r,s,c,t in circle_rows:
    a=math.radians(d)
    assert abs(val(r.replace('π','*math.pi').lstrip('*') if r!='π' else 'math.pi')-a)<1e-9 if not r[0] in 'π' else True
    assert abs(val(s)-math.sin(a))<1e-9,(d,s); assert abs(val(c)-math.cos(a))<1e-9,(d,c)
    if t!='undefined': assert abs(val(t)-math.tan(a))<1e-9,(d,t)
circle=SH('unit circle','Unit Circle','unit circle',
 S('Unit circle',G('Every 30° and 45°',{'show':' ','copy':'','figure':'FIGURE'}),wide=True),
 S('Exact values',
  G('Every 30° and 45°',H('Degrees','Radians','sin','cos','tan'),*[C(f'{d}°',r,s,c,t) for d,r,s,c,t in circle_rows]),
  wide=True,leads=[80,80,80,80]),
 S('Reading the circle',
  G('Any angle',
    I('(x, y) = (cos θ, sin θ)',note='the point at angle θ'),
    I('θ_{rad} = θ_{deg} π/180',note='degrees to radians'),
    I('tan x = sin x / cos x',copy='tan(x) = sin(x)/cos(x)'),
    I('sin^{2}x + cos^{2}x = 1',note='Pythagorean identity',copy='sin(x)^2 + cos(x)^2 = 1'),
    I('1 + tan^{2}x = sec^{2}x',copy='1 + tan(x)^2 = sec(x)^2')),
  G('Symmetry and reference angles (positive: All, Sin, Tan, Cos)',
    I('θ_{ref} = 180° − θ,  θ − 180°,  360° − θ',note='reference angle in quadrant II, III, IV',copy='180° - θ'),
    I('sin(π − x) = sin x',note='quadrant II: same sine',copy='sin(pi - x) = sin(x)'),
    I('cos(π − x) = −cos x',note='quadrant II: cosine flips',copy='cos(pi - x) = -cos(x)'),
    I('sin(−x) = −sin x',note='odd',copy='sin(-x) = -sin(x)'),
    I('cos(−x) = cos x',note='even',copy='cos(-x) = cos(x)'),
    I('sin(x + 2π) = sin x',note='period 2π; tan has period π',copy='sin(x + 2pi) = sin(x)')),
  G('Inverse functions: the calculator range',
    I('arcsin x ∈ [−π/2, π/2]',note='quadrants I and IV',copy='arcsin(x)'),
    I('arccos x ∈ [0, π]',note='quadrants I and II',copy='arccos(x)'),
    I('arctan x ∈ (−π/2, π/2)',note='quadrants I and IV; use atan2 for a point',copy='arctan(x)')),
  G('Reciprocals',
    I('csc x = 1/sin x',copy='csc(x) = 1/sin(x)'),
    I('sec x = 1/cos x',copy='sec(x) = 1/cos(x)'),
    I('cot x = 1/tan x',copy='cot(x) = 1/tan(x)')),
  wide=True))

geom=SH('geometry','Geometry Formulas','geometry formulas',
 S('Area and perimeter',
  G('Circles and ellipses',
    I('A = πr^{2}',note='circle area'),
    I('C = 2πr',note='circle circumference'),
    I('A = (1/2) r^{2} θ',note='sector area, θ in radians'),
    I('s = rθ',note='arc length, θ in radians'),
    I('c = 2r sin(θ/2)',note='chord length, central angle θ'),
    I('A = π(R^{2} − r^{2})',note='annulus (ring, washer) area'),
    I('A = πab',note='ellipse area, semi-axes a and b')),
  G('Rectangles, triangles and polygons',
    I('A = l w',note='rectangle'),
    I('P = 2(l + w)',note='rectangle perimeter'),
    I('A = (1/2) b h',note='triangle, base and height'),
    I('A = (1/2) a b sin(C)',note='triangle, C between a and b'),
    I('A = √(s(s − a)(s − b)(s − c))',note='Heron, s = semiperimeter'),
    I('A = (√3/4) a^{2}',note='equilateral triangle'),
    I('A = (1/2)(a + b) h',note='trapezoid, parallel sides a and b'),
    I('A = b h',note='parallelogram'),
    I('A = (1/2) d_{1} d_{2}',note='rhombus or kite, diagonals d₁ and d₂'),
    I('A = (1/4) n s^{2} cot(π/n)',note='regular n-gon, side s'))),
 S('Surface and volume',
  G('Volume',
    I('V = l w h',note='box, rectangular prism'),
    I('V = B h',note='prism, base area B'),
    I('V = πr^{2} h',note='cylinder'),
    I('V = (1/3) B h',note='pyramid, base area B'),
    I('V = (1/3) πr^{2} h',note='cone'),
    I('V = (4/3) πr^{3}',note='sphere'),
    I('V = (1/3) πh(R^{2} + R r + r^{2})',note='frustum, cut-off cone')),
  G('Surface area',
    I('S = 2(l w + l h + w h)',note='box'),
    I('S = 2πr^{2} + 2πrh',note='closed cylinder'),
    I('S = πr^{2} + πr√(r^{2} + h^{2})',note='cone: base plus slant side'),
    I('S = 4πr^{2}',note='sphere'))),
 S('Triangles and angles',
  G('Triangles',
    I('a^{2} + b^{2} = c^{2}',note='Pythagorean theorem, right triangle'),
    I('c^{2} = a^{2} + b^{2} − 2ab cos(C)',note='law of cosines'),
    I('a/sin(A) = b/sin(B) = c/sin(C)',note='law of sines'),
    I('A + B + C = 180°',note='angle sum of a triangle')),
  G('Special right triangles and polygons',
    I('1 : 1 : √2',note='45-45-90 triangle sides'),
    I('1 : √3 : 2',note='30-60-90 triangle sides'),
    I('(n − 2) 180°',note='interior angle sum, n-gon'),
    I('360°/n',note='each exterior angle of a regular n-gon')),
  G('Similar figures and circle angles',
    I('A_{2}/A_{1} = k^{2},  V_{2}/V_{1} = k^{3}',note='similar figures, length scale factor k'),
    I('θ_{i} = θ_{c}/2',note='inscribed angle, central θ_c'))),
 S('Coordinates',
  G('Points',
    I('d = √((x_{2} − x_{1})^{2} + (y_{2} − y_{1})^{2})',note='distance'),
    I('d = √((x_{2} − x_{1})^{2} + (y_{2} − y_{1})^{2} + (z_{2} − z_{1})^{2})',note='3D'),
    I('((x_{1} + x_{2})/2, (y_{1} + y_{2})/2)',note='midpoint')),
  G('Lines and circles',
    I('m = (y_{2} − y_{1})/(x_{2} − x_{1})',note='slope'),
    I('y = m x + b',note='slope-intercept form'),
    I('y − y_{1} = m(x − x_{1})',note='point-slope form'),
    I('m_{1} m_{2} = −1',note='perpendicular; parallel: m₁ = m₂'),
    I('d = (|a x_{0} + b y_{0} + c|)/√(a^{2} + b^{2})',note='point to the line ax + by + c = 0'),
    I('(x − h)^{2} + (y − k)^{2} = r^{2}',note='circle, centre (h, k)'))))

stats=SH('statistics','Statistics and Probability','statistics and probability',
 S('Describing data',
  G('One variable',
    I('x̄ = (1/n) Σ x_{i}',note='mean, average'),
    I('s^{2} = Σ (x_{i} − x̄)^{2} / (n − 1)',note='sample variance'),
    I('s = √(Σ (x_{i} − x̄)^{2}/(n − 1))',note='sample standard deviation'),
    I('σ^{2} = Σ (x_{i} − μ)^{2}/N',note='population variance, divide by N'),
    I('σ_{x̄} = σ/√n',note='standard error of the mean, SE'),
    I('z = (x − μ)/σ',note='z-score, standard score'),
    I('IQR = Q_{3} − Q_{1}',note='interquartile range, outlier rule')),
  G('Two variables',
    I('σ_{XY} = E[XY] − E[X] E[Y]',note='covariance, Cov(X, Y)'),
    I('r = σ_{XY}/(σ_{X} σ_{Y})',note='correlation coefficient, −1 ≤ r ≤ 1'),
    I('b_{1} = r s_{y}/s_{x}',note='least-squares regression slope'),
    I('b_{0} = ȳ − b_{1} x̄',note='regression intercept, ŷ = b₀ + b₁x')),
  G('Expectation',
    I('E[X] = Σ x p(x)',note='expected value, discrete'),
    I('E[X] = ∫ x f(x) dx',note='expected value, continuous'),
    I('Var(X) = E[X^{2}] − E[X]^{2}',note='variance'),
    I('E[aX + b] = a E[X] + b',note='expectation is linear'),
    I('Var(aX + b) = a^{2} Var(X)',note='a shift adds no variance'),
    I('Var(X + Y) = Var(X) + Var(Y) + 2σ_{XY}'))),
 S('Probability',
  G('Rules',
    I('P(A^{c}) = 1 − P(A)',note='complement'),
    I('P(A ∪ B) = P(A) + P(B) − P(A ∩ B)',note='addition rule'),
    I('P(A ∩ B) = P(A) P(B)',note='independent events, and'),
    I('P(A | B) = P(A ∩ B)/P(B)',note='conditional probability'),
    I('P(B) = Σ P(B | A_{i}) P(A_{i})',note='law of total probability'),
    I('P(A | B) = P(B | A) P(A)/P(B)',note="Bayes' theorem")),
  G('Counting',
    I('nPr = n!/(n − r)!',note='permutations: order matters'),
    I('nCr = n!/(r!(n − r)!)',note='combinations: order doesn’t matter'))),
 S('Distributions',
  G('Name, then its probability, mean and variance',
    H('Name','Probability','Mean, variance'),
    L('Binomial','nCk p^{k} (1 − p)^{n−k}','np, np(1 − p)'),
    L('Poisson','λ^{k} e^{−λ}/k!','λ, λ'),
    L('Geometric','(1 − p)^{k−1} p','1/p, (1 − p)/p^{2}'),
    L('Uniform','1/(b − a)','(a + b)/2, (b − a)^{2}/12'),
    L('Exponential','λ e^{−λx}','1/λ, 1/λ^{2}'),
    L('Normal','e^{−(x − μ)^{2}/(2σ^{2})} / (σ√(2π))','μ, σ^{2}')),
  wide=True,leads=[100,280]),
 S('The normal curve',
  G('The 68-95-99.7 (empirical) rule',
    I('within 1σ: 68.27%',copy='68.27%'),
    I('within 2σ: 95.45%',copy='95.45%'),
    I('within 3σ: 99.73%',copy='99.73%'))),
 S('Inference',
  G('Confidence intervals',
    I('x̄ ± z σ/√n',note='mean, σ known'),
    I('x̄ ± t s/√n',note='mean, σ unknown, n − 1 df'),
    I('p̂ ± z √(p̂(1 − p̂)/n)',note='proportion'),
    I('n = (z σ/E)^{2}',note='sample size for a margin of error E')),
  G('Critical z, two-sided',
    I('z = 1.645',note='90% confidence (95% one-sided)',copy='1.645'),
    I('z = 1.960',note='95% confidence',copy='1.96'),
    I('z = 2.576',note='99% confidence',copy='2.576')),
  G('Hypothesis tests',
    I('z = (x̄ − μ_{0})/(σ/√n)',note='z test for a mean'),
    I('t = (x̄ − μ_{0})/(s/√n)',note='t test, n − 1 df'),
    I('z = (p̂ − p_{0})/√(p_{0}(1 − p_{0})/n)',note='z test for a proportion'),
    I('t = (x̄_{1} − x̄_{2})/√(s_{1}^{2}/n_{1} + s_{2}^{2}/n_{2})',note='two-sample (Welch) t test'),
    I('χ^{2} = Σ (O − E)^{2}/E',note='chi-square, observed vs expected'))))

linalg=SH('linear algebra','Linear Algebra','linear algebra',
 S('Vectors',
  G('Products',
    I('a · b = Σ a_{i} b_{i} = |a| |b| cos(θ)',note='dot product'),
    I('a × b = (a_{2}b_{3} − a_{3}b_{2}, a_{3}b_{1} − a_{1}b_{3}, a_{1}b_{2} − a_{2}b_{1})'),
    I('|a × b| = |a| |b| sin(θ)',note='area of the parallelogram'),
    I('a · (b × c) = det[a; b; c]',note='scalar triple product: volume')),
  G('Length, angle and projection',
    I('|a| = √(a · a)',note='magnitude, norm'),
    I('u = a/|a|',note='unit vector along a'),
    I('cos(θ) = (a · b)/(|a| |b|)',note='angle between a and b'),
    I('proj_{b} a = ((a · b)/(b · b)) b',note='projection of a onto b'))),
 S('Matrices',
  G('Products and inverses',
    I('(AB)_{ij} = Σ_{k} a_{ik} b_{kj}',note='matrix multiplication'),
    I('A A^{−1} = A^{−1} A = I',note='inverse, exists when det A ≠ 0'),
    I('x = A^{−1} b',note='solves A x = b'),
    I('x_{i} = det(A_{i})/det(A)',note="Cramer: b replaces column i")),
  G('Small matrices: A = [a b; c d], then [a b c; d e f; g h i]',
    I('det A = ad − bc',note='determinant, 2 × 2'),
    I('A^{−1} = (1/det A) [d −b; −c a]',note='inverse, 2 × 2'),
    I('det A = a(ei − fh) − b(di − fg) + c(dh − eg)'),
    I('A^{−1} = C^{T}/det(A)',note='adjugate: Cᵢⱼ = (−1)^{i+j} Mᵢⱼ, cofactors')),
  G('Rules',
    I('(AB)^{T} = B^{T} A^{T}',note='transpose of a product'),
    I('(AB)^{−1} = B^{−1} A^{−1}',note='inverse of a product'),
    I('det(AB) = det(A) det(B)'),
    I('det(A^{T}) = det(A)'),
    I('det(A^{−1}) = 1/det(A)'),
    I('det(cA) = c^{n} det(A)',note='A is n × n')),
  G('Special matrices',
    I('A^{T} = A',note='symmetric: real eigenvalues'),
    I('Q^{T} Q = I',note='orthogonal: Q^{−1} = Q^{T}'))),
 S('Solving and least squares',
  G('Solutions of A x = b',
    I('R_{i} ← R_{i} − (a_{ik}/a_{kk}) R_{k}',note='Gaussian elimination: zero below pivot a_kk'),
    I('rank(A) = rank([A | b])',note='A x = b is consistent; one solution if rank = n'),
    I('rank(A) + nullity(A) = n',note='rank-nullity, n columns')),
  G('Least squares and projection',
    I('A^{T} A x = A^{T} b',note='least squares: the normal equations'),
    I('x = (A^{T} A)^{−1} A^{T} b',note='least squares solution, independent columns'),
    I('P = A(A^{T} A)^{−1} A^{T}',note='projection onto the column space: p = P b')),
  G('Orthogonal bases',
    I('u_{k} = v_{k} − Σ_{j<k} ((v_{k} · u_{j})/(u_{j} · u_{j})) u_{j}',note='Gram–Schmidt, then normalize each u'),
    I('A = Q R',note='QR: Q orthonormal columns, R upper triangular; R x = Qᵀb')),
  wide=True),
 S('Eigenvalues',
  G('Finding them',
    I('A v = λ v',note='eigenvector v, eigenvalue λ'),
    I('det(A − λI) = 0',note='characteristic equation'),
    I('(A − λI) v = 0',note='eigenvectors: the null space of A − λI, for each λ'),
    I('λ = T/2 ± √(T^{2}/4 − D)',note='2 × 2, T = trace, D = det'),
    I('λ^{3} − tr(A) λ^{2} + (M_{11} + M_{22} + M_{33}) λ − det A = 0',note='3 × 3: Mᵢᵢ are the principal 2 × 2 minors'),
    I('λ_{i} = a_{ii}',note='triangular or diagonal A: the diagonal')),
  G('Checks',
    I('tr(A) = Σ λ_{i}',note='trace'),
    I('det(A) = Π λ_{i}',note='determinant')),
  G('Diagonalizing',
    I('A = P D P^{−1}',note='P: eigenvectors, D: eigenvalues'),
    I('A^{k} = P D^{k} P^{−1}',note='matrix powers'),
    I('e^{At} = P e^{Dt} P^{−1}',note='matrix exponential: x′ = A x gives x = e^{At} x(0)'),
    I('A = U Σ V^{T}',note='SVD: σᵢ = √(eigenvalues of AᵀA)')),
  wide=True))

physics=SH('physics','Physics Formulas','physics formulas',
 S('Mechanics',
  G('Constant acceleration',
    I('v = v_{0} + a t'),
    I('x = x_{0} + v_{0} t + (1/2) a t^{2}'),
    I('v^{2} = v_{0}^{2} + 2a Δx',copy='v^2 = v_0^2 + 2a Δx'),
    I('Δx = (1/2)(v_{0} + v) t')),
  G('Projectiles',
    I('x = v_{0} cos(θ) t',note='horizontal'),
    I('y = y_{0} + v_{0} sin(θ) t − (1/2) g t^{2}',note='height'),
    I('R = v_{0}^{2} sin(2θ)/g',note='range, level ground'),
    I('H = v_{0}^{2} sin^{2}(θ)/(2g)',note='maximum height',copy='H = v_0^2 sin(θ)^2/(2g)'),
    I('t = 2 v_{0} sin(θ)/g',note='time of flight, level ground')),
  G('Force and energy',
    I('F = m a',note="Newton's second law"),
    I('f ≤ μ N',note='friction: μs static, μk kinetic'),
    I('W = F d cos(θ)',note='work'),
    I('W = Δ KE',note='work-energy theorem'),
    I('KE = (1/2) m v^{2}',note='kinetic energy'),
    I('PE = m g h',note='potential energy'),
    I('P = dW/dt = F v',note='power')),
  G('Momentum',
    I('p = m v',note='momentum'),
    I('∫ F dt = Δp',note='impulse'),
    I('m_{1} v_{1} + m_{2} v_{2} = m_{1} v′_{1} + m_{2} v′_{2}',note='momentum conserved')),
  G('Circular motion and rotation',
    I('v = ω r',note='speed at radius r; rolling'),
    I('a_{c} = v^{2}/r',note='centripetal acceleration'),
    I('F_{c} = m v^{2}/r',note='centripetal force'),
    I('tan(θ) = v^{2}/(r g)',note='banked curve, no friction needed'),
    I('τ = r F sin(θ)',note='torque'),
    I('τ = I α',note="Newton's second law for rotation"),
    I('L = I ω',note='angular momentum'),
    I('KE = (1/2) I ω^{2}',note='rotational kinetic energy')),
  G('Gravity and orbits',
    I('F = G m_{1} m_{2}/r^{2}',note="Newton's law of gravitation"),
    I('g = G M/r^{2}',note='surface gravity'),
    I('v = √(G M/r)',note='circular orbit speed'),
    I('v_{e} = √(2 G M/r)',note='escape velocity'),
    I('T^{2} = 4π^{2} a^{3}/(G M)',note="Kepler's third law: orbit period")),
  G('Springs and oscillation',
    I('F = −k x',note="Hooke's law"),
    I('PE = (1/2) k x^{2}',note='spring energy'),
    I('x(t) = A cos(ωt + φ)',note='simple harmonic motion, SHM'),
    I('ω = √(k/m)',note='mass on a spring'),
    I('T = 2π√(m/k)',note='period, mass on a spring'),
    I('T = 2π√(L/g)',note='period, simple pendulum'))),
 S('Fluids',
  G('Pressure and flow',
    I('p = F/A',note='pressure'),
    I('p = p_{0} + ρgh',note='hydrostatic pressure at depth h'),
    I('F_{b} = ρgV',note='buoyancy, Archimedes'),
    I('ṁ = ρ_{1} A_{1} v_{1} = ρ_{2} A_{2} v_{2}',note='continuity'),
    I('p + ρv^{2}/2 + ρgz = constant',note="Bernoulli's equation"))),
 S('Waves, sound and optics',
  G('Waves',
    I('v = f λ',note='wave speed'),
    I('T = 1/f',note='period and frequency'),
    I('v = √(T/μ)',note='wave on a string: T tension, μ mass per length'),
    I('f_{n} = n v/(2L)',note='string or open pipe, harmonics'),
    I('f_{n} = n v/(4L)',note='pipe closed at one end, n odd')),
  G('Sound',
    I('f′ = f (v + v_{o})/(v − v_{s})',note='Doppler: o observer, s source'),
    I('β = 10 log_{10}(I/I_{0})',note='decibels, sound level'),
    I('I = P/(4πr^{2})',note='intensity, inverse square')),
  G('Optics',
    I('n = c/v',note='refractive index'),
    I('n_{1} sin(θ_{1}) = n_{2} sin(θ_{2})',note="Snell's law"),
    I('sin(θ_{c}) = n_{2}/n_{1}',note='critical angle'),
    I('1/f = 1/d_{o} + 1/d_{i}',note='thin lens and mirror'),
    I('m = −d_{i}/d_{o}',note='magnification'),
    I('d sin(θ) = m λ',note='double slit or grating, bright fringes'))),
 S('Thermodynamics',
  G('Heat',
    I('Q = m c ΔT',note='specific heat'),
    I('Q = m L',note='latent heat, change of phase'),
    I('ΔL = α L ΔT',note='thermal expansion'),
    I('Q/t = k A ΔT/d',note='conduction'),
    I('P = e σ A T^{4}',note='radiation, Stefan-Boltzmann')),
  G('Gases',
    I('P V = n R T',note='ideal gas law'),
    I('U = (3/2) n R T',note='monatomic ideal gas'),
    I('v_{rms} = √(3RT/M)',note='rms molecular speed'),
    I('P V^{γ} = constant',note='adiabatic')),
  G('Laws and engines',
    I('ΔU = Q − W',note='first law'),
    I('ΔS = Q/T',note='entropy, reversible'),
    I('W = P ΔV',note='constant pressure'),
    I('W = n R T ln(V_{2}/V_{1})',note='isothermal'),
    I('η = W/Q_{h}',note='efficiency of any engine'),
    I('η = 1 − T_{c}/T_{h}',note='Carnot efficiency'))),
 S('Electricity and magnetism',
  G('Charges and fields',
    I('F = k q_{1} q_{2}/r^{2}',note="Coulomb's law"),
    I('E = kq/r^{2}',note='field of a point charge'),
    I('V = kq/r',note='potential of a point charge'),
    I('U = qV',note='potential energy'),
    I('E = −dV/dx',note='field from potential'),
    I('C = ε_{0} A/d',note='parallel plates'),
    I('U = (1/2) C V^{2}',note='energy in a capacitor')),
  G('Current and DC circuits',
    I('V = I R',note="Ohm's law"),
    I('P = V I = I^{2} R = V^{2}/R',note='electric power'),
    I('R = R_{1} + R_{2}',note='resistors in series'),
    I('1/R = 1/R_{1} + 1/R_{2}',note='resistors in parallel'),
    I('q = Q (1 − e^{−t/(RC)})',note='RC charging a capacitor, τ = RC'),
    I('R = ρL/A',note='resistivity'),
    I('I = n q A v_{d}',note='drift speed')),
  G('Magnetism',
    I('F = q(E + v × B)',note='Lorentz force'),
    I('F = q v B sin(θ)',note='on a moving charge'),
    I('r = m v/(q B)',note='radius of a charge circling in B'),
    I('F = I L B sin(θ)',note='on a wire'),
    I('B = μ_{0} I/(2πr)',note='long straight wire'),
    I('B = μ_{0} n I',note='solenoid'),
    I('Φ = B A cos(θ)',note='magnetic flux'),
    I('ε = −N dΦ/dt',note="Faraday's law, induced emf")),
  G("Maxwell's equations",
    I('∇ · E = ρ/ε_{0}',note="Gauss's law"),
    I('∇ · B = 0',note='no magnetic monopoles'),
    I('∇ × E = −∂B/∂t',note="Faraday's law"),
    I('∇ × B = μ_{0} J + μ_{0} ε_{0} ∂E/∂t',note='Ampère-Maxwell law'))),
 S('Relativity, quantum and nuclear',
  G('Relativity',
    I('γ = 1/√(1 − v^{2}/c^{2})',note='Lorentz factor'),
    I('t = γ t_{0}',note='time dilation'),
    I('L = L_{0}/γ',note='length contraction'),
    I('E = m c^{2}',note='rest energy'),
    I('E^{2} = (pc)^{2} + (mc^{2})^{2}',note='energy and momentum')),
  G('Quantum',
    I('E = h f',note='photon energy'),
    I('E = h c/λ',note='photon energy from wavelength'),
    I('λ = h/p',note='de Broglie'),
    I('KE = h f − φ',note='photoelectric effect'),
    I('Δx Δp ≥ ħ/2',note='uncertainty principle'),
    I('E_{n} = −13.6 eV/n^{2}',note='hydrogen energy levels'),
    I('iħ ∂ψ/∂t = H ψ',note='Schrödinger')),
  G('Nuclear',
    I('N = N_{0} e^{−λt}',note='radioactive decay'),
    I('t_{1/2} = ln(2)/λ',note='half-life'),
    I('E = Δm c^{2}',note='binding energy, mass defect'))))

chemistry=SH('chemistry','Chemistry Formulas','chemistry formulas: stoichiometry, solutions, gases, thermochemistry, equilibrium, acids, kinetics, electrochemistry',
 S('Amounts and solutions',
  G('Moles and stoichiometry',
    I('n = m/M',note='moles from mass, M molar mass'),
    I('n_{B} = (b/a) n_{A}',note='mole ratio, aA → bB'),
    I('N = n N_{A}',note='number of particles, Avogadro'),
    I('w = 100 × m_{part}/m_{total}',note='mass percent, percent composition'),
    I('percent yield = 100 × actual/theoretical',note='percent yield')),
  G('Concentration',
    I('c = n/V',note='molarity, mol per litre'),
    I('b = n/m_{solvent}',note='molality, mol per kg of solvent'),
    I('x_{i} = n_{i}/n_{tot}',note='mole fraction'),
    I('c_{1} V_{1} = c_{2} V_{2}',note='dilution'),
    I('A = ε l c',note='Beer-Lambert'))),
 S('Gas laws',
  G('Ideal',
    I('P V = n R T',note='ideal gas law'),
    I('ρ = P M/(R T)',note='gas density; solve for molar mass M'),
    I('P_{1} V_{1}/T_{1} = P_{2} V_{2}/T_{2}',note='combined gas law'),
    I('P_{1} V_{1} = P_{2} V_{2}',note="Boyle's law: constant T"),
    I('V_{1}/T_{1} = V_{2}/T_{2}',note="Charles's law: constant P"),
    I('P_{1}/T_{1} = P_{2}/T_{2}',note="Gay-Lussac's law: constant V"),
    I('P_{tot} = Σ P_{i}',note="Dalton's law of partial pressures"),
    I('P_{i} = x_{i} P_{tot}',note='partial pressure from mole fraction')),
  G('Real, and moving',
    I('(P + a n^{2}/V^{2})(V − n b) = n R T',note='van der Waals'),
    I('r_{1}/r_{2} = √(M_{2}/M_{1})',note="Graham's law of effusion"),
    I('v_{rms} = √(3RT/M)',note='root-mean-square speed, M in kg/mol'))),
 S('Thermochemistry',
  G('Heat and free energy',
    I('q = m c ΔT',note='heat, specific heat c (calorimetry)'),
    I('ΔH = Σ BE(broken) − Σ BE(formed)',note='enthalpy from bond energies'),
    I('ΔH° = Σ ΔH°_{f}(products) − Σ ΔH°_{f}(reactants)',note='Hess'),
    I('ΔS° = Σ S°(products) − Σ S°(reactants)',note='standard entropy change'),
    I('ΔG = ΔH − T ΔS',note='Gibbs free energy: negative is spontaneous'),
    I('ΔG° = −R T ln(K)',note='free energy and the equilibrium constant'),
    I('ΔG = ΔG° + R T ln(Q)',note='free energy away from equilibrium'),
    I('ln(P_{2}/P_{1}) = −(ΔH_{vap}/R)(1/T_{2} − 1/T_{1})',note='Clausius-Clapeyron')),
  wide=True),
 S('Equilibrium, acids and bases',
  G('Equilibrium',
    I('K = ([C]^{c} [D]^{d})/([A]^{a} [B]^{b})',note='aA + bB ⇌ cC + dD'),
    I('K_{p} = K_{c} (RT)^{Δn}',note='Δn = moles of gas, products − reactants'),
    I('Q < K',note='reaction quotient: runs forward'),
    I('ln(K_{2}/K_{1}) = −(ΔH°/R)(1/T_{2} − 1/T_{1})',note="van 't Hoff"),
    I('c = k_{H} P',note="Henry's law: gas solubility"),
    I('K_{sp} = [A]^{a} [B]^{b}',note='solubility product')),
  G('Acids and bases',
    I('pH = −log_{10}[H^{+}]',note='acidity'),
    I('pOH = −log_{10}[OH^{−}]',note='basicity'),
    I('pH + pOH = 14',note='25 °C'),
    I('K_{a} = [H^{+}][A^{−}]/[HA]',note='acid dissociation, pKa = −log Ka'),
    I('[H^{+}] ≈ √(K_{a} c)',note='weak acid of concentration c'),
    I('K_{a} K_{b} = K_{w} = 10^{−14}',note='conjugate pair, 25 °C'),
    I('pH = pK_{a} + log_{10}([A^{−}]/[HA])',note='Henderson-Hasselbalch, buffer'),
    I('c_{a} V_{a} = c_{b} V_{b}',note='titration'))),
 S('Kinetics',
  G('Rate laws',
    H('Order','Integrated','Half-life'),
    L('0','[A] = [A]_{0} − k t','[A]_{0}/(2k)'),
    L('1','ln[A] = ln[A]_{0} − k t','ln(2)/k'),
    L('2','1/[A] = 1/[A]_{0} + k t','1/(k[A]_{0})')),
  G('Rate and temperature',
    I('rate = k [A]^{m} [B]^{n}',note='rate law, orders m and n'),
    I('k = A e^{−E_{a}/(RT)}',note='Arrhenius'),
    I('ln(k_{2}/k_{1}) = −(E_{a}/R)(1/T_{2} − 1/T_{1})',note='Arrhenius, two temperatures'),
    I('ln(k) = ln(A) − E_{a}/(R T)',note='Arrhenius plot: ln k against 1/T has slope −Ea/R')),
  wide=True,leads=[60,240]),
 S('Electrochemistry',
  G('Cells',
    I('E°_{cell} = E°_{cathode} − E°_{anode}',note='standard cell potential'),
    I('ΔG = −n F E',note='free energy from cell potential'),
    I('E° = (RT/(nF)) ln(K)',note='cell potential and the equilibrium constant'),
    I('E = E° − (RT/(nF)) ln(Q)',note='Nernst'),
    I('E = E° − (0.0592/n) log_{10}(Q)',note='Nernst, 25 °C'),
    I('m = I t M/(n F)',note="Faraday's law of electrolysis, mass plated")),
  wide=True),
 S('Colligative properties and light',
  G('Solutions',
    I('ΔT_{b} = i K_{b} b',note='boiling point rises'),
    I('ΔT_{f} = i K_{f} b',note='freezing point falls'),
    I('π = i c R T',note='osmotic pressure'),
    I('P = x P°',note="Raoult's law, vapour pressure")),
  G('Light and atoms',
    I('E = h c/λ',note='photon energy'),
    I('c = λ ν',note='speed of light = wavelength × frequency'),
    I('E_{n} = −13.6 eV/n^{2}',note='hydrogen energy levels, Bohr'),
    I('1/λ = R_{H} (1/n_{1}^{2} − 1/n_{2}^{2})',note='Rydberg, hydrogen spectrum'),
    I('FC = V − N − B/2',note='formal charge'))))

biology=SH('biology','Biology Formulas','biology formulas: populations, ecology, genetics, enzymes, membranes, metabolism, physiology, pharmacology, lab',
 S('Populations and ecology',
  G('Growth',
    I('dN/dt = r N',note='exponential growth'),
    I('N = N_{0} e^{rt}',note='exponential growth, solved'),
    I('dN/dt = r N (1 − N/K)',note='logistic growth, carrying capacity K'),
    I('N_{t} = N_{0} λ^{t}',note='geometric growth, λ = N_{t+1}/N_{t}'),
    I('t_{d} = ln(2)/r',note='doubling time'),
    I('t_{d} ≈ 70/r',note='rule of 70, r in %'),
    I('N = M C/R',note='mark-recapture (Lincoln-Petersen)')),
  G('Communities and energy',
    I('dN/dt = r N − a N P',note='Lotka-Volterra, prey'),
    I('dP/dt = b N P − m P',note='Lotka-Volterra, predator'),
    I('NPP = GPP − R',note='net primary productivity'),
    I('E = P_{n+1}/P_{n}',note='trophic (ecological) efficiency, about 10%')),
  G('Diversity',
    I('H = −Σ p_{i} ln(p_{i})',note='Shannon diversity index'),
    I('D = 1 − Σ p_{i}^{2}',note='Simpson diversity index'))),
 S('Genetics',
  G('Allele frequencies',
    I('p + q = 1',note='Hardy-Weinberg, allele frequencies'),
    I('p^{2} + 2pq + q^{2} = 1',note='Hardy-Weinberg, genotype frequencies')),
  G('Tests and traits',
    I('χ^{2} = Σ (O − E)^{2}/E',note='chi-square test, df = categories − 1'),
    I('G = 2^{n}',note='kinds of gametes, n heterozygous genes'),
    I('h^{2} = V_{A}/V_{P}',note='narrow-sense heritability'),
    I('R = h^{2} S',note="breeder's equation: response to selection"),
    I('RF = recombinants/total',note='recombination: 1% = 1 cM'))),
 S('Enzymes',
  G('Kinetics',
    I('v = V_{max} [S]/(K_{m} + [S])',note='Michaelis-Menten'),
    I('1/v = (K_{m}/V_{max})(1/[S]) + 1/V_{max}',note='Lineweaver-Burk'),
    I('k_{cat} = V_{max}/[E]',note='turnover number'),
    I('K_{m,app} = K_{m} (1 + [I]/K_{i})',note='competitive inhibition: apparent Km rises'),
    I('V_{max,app} = V_{max}/(1 + [I]/K_{i})',note='noncompetitive inhibition: apparent Vmax falls'),
    I('θ = [L]^{n}/(K_{A}^{n} + [L]^{n})',note='Hill equation: K_A gives half occupancy, cooperativity n')),
  wide=True),
 S('Membranes and transport',
  G('Potentials',
    I('E = (RT/(zF)) ln([X]_{out}/[X]_{in})',note='Nernst'),
    I('E = (61.5 mV/z) log_{10}([X]_{out}/[X]_{in})',note='Nernst, 37 °C'),
    I('V_{m} = (RT/F) ln((P_{K}[K^{+}]_{o} + P_{Na}[Na^{+}]_{o} + P_{Cl}[Cl^{−}]_{i})/(P_{K}[K^{+}]_{i} + P_{Na}[Na^{+}]_{i} + P_{Cl}[Cl^{−}]_{o}))',note='Goldman (GHK) resting membrane potential')),
  G('Moving across',
    I('J = −D dC/dx',note="Fick's law of diffusion"),
    I('SA/V = 3/r',note='surface area to volume ratio, sphere'),
    I('Ψ = Ψ_{s} + Ψ_{p}',note='water potential'),
    I('Ψ_{s} = −i C R T',note='solute potential')),
  wide=True),
 S('Metabolism',
  G('Overall reactions',
    I('6CO_{2} + 6H_{2}O → C_{6}H_{12}O_{6} + 6O_{2}',note='photosynthesis'),
    I('C_{6}H_{12}O_{6} + 6O_{2} → 6CO_{2} + 6H_{2}O',note='respiration')),
  G('Energy and temperature',
    I('ΔG°′ = −30.5 kJ/mol',note='ATP hydrolysis'),
    I('Q_{10} = (R_{2}/R_{1})^{10/(T_{2} − T_{1})}',note='temperature coefficient: rate change per 10 °C'),
    I('RQ = V_{CO_{2}}/V_{O_{2}}',note='respiratory quotient: 1.0 carbohydrate, 0.7 fat')),
  wide=True),
 S('Physiology and pharmacology',
  G('Body',
    I('CO = HR × SV',note='cardiac output'),
    I('SV = EDV − ESV',note='stroke volume'),
    I('EF = SV/EDV',note='ejection fraction'),
    I('MAP = CO × TPR',note='mean arterial pressure'),
    I('MAP ≈ DBP + (SBP − DBP)/3',note='MAP from cuff pressures'),
    I('Q = πR^{4}ΔP/(8ηL)',note='Poiseuille'),
    I('BMI = m/h^{2}',note='body mass index, kg and metres')),
  G('Drugs',
    I('C = C_{0} e^{−kt}',note='first-order elimination'),
    I('t_{1/2} = ln(2)/k',note='half-life'),
    I('V_{d} = dose/C_{0}',note='volume of distribution'),
    I('CL = k V_{d}',note='clearance'),
    I('LD = C_{target} V_{d}',note='loading dose'))),
 S('Lab',
  G('Solutions and counts',
    I('c_{1} V_{1} = c_{2} V_{2}',note='dilution'),
    I('A = ε l c',note='Beer-Lambert'),
    I('N = N_{0} 2^{n}',note='PCR, n cycles'),
    I('N = N_{0} 2^{t/g}',note='bacterial growth, generation time g'),
    I('M = I/A',note='magnification: image size over actual size'))))

circuits=SH('circuits','Electrical Engineering','electrical engineering',
 S('Basics',
  G('Laws',
    I('V = I R',note="Ohm's law"),
    I('P = V I = I^{2} R = V^{2}/R',note='power'),
    I('Σ I = 0',note='KCL, at a node'),
    I('Σ V = 0',note='KVL, around a loop')),
  G('Dividers',
    I('V_{out} = V_{in} R_{2}/(R_{1} + R_{2})',note='voltage divider'),
    I('I_{1} = I R_{2}/(R_{1} + R_{2})',note='current divider')),
  G('Combining',
    H('Part','Series','Parallel'),
    L('R','R_{1} + R_{2}','R_{1}R_{2}/(R_{1} + R_{2})'),
    L('L','L_{1} + L_{2}','L_{1}L_{2}/(L_{1} + L_{2})'),
    L('C','C_{1}C_{2}/(C_{1} + C_{2})','C_{1} + C_{2}')),
  G('Networks',
    I('V_{out} = V_{s} (R_{2}/(R_{1} + R_{2}) − R_{4}/(R_{3} + R_{4}))',note='Wheatstone bridge'),
    I('R_{1} = R_{b} R_{c}/(R_{a} + R_{b} + R_{c})',note='delta to wye'),
    I('R_{a} = (R_{1}R_{2} + R_{2}R_{3} + R_{3}R_{1})/R_{1}',note='wye to delta')),
  G('Thévenin and Norton',
    I('V_{th} = V_{oc}',note='Thévenin voltage: open circuit'),
    I('R_{th} = V_{oc}/I_{sc}',note='Thévenin resistance'),
    I('I_{N} = I_{sc}',note='Norton current: short circuit'),
    I('R_{N} = R_{th}',note='Norton resistance'),
    I('V_{s} = I_{s} R',note='source transformation')),
  leads=[40,130]),
 S('Capacitors and inductors',
  G('Storing energy',
    I('Q = C V',note='capacitor charge'),
    I('i = C dv/dt',note='capacitor current'),
    I('v = L di/dt',note='inductor voltage'),
    I('E = (1/2) C V^{2}',note='capacitor energy'),
    I('E = (1/2) L I^{2}',note='inductor energy')),
  G('First order (RC, RL)',
    I('τ = R C',note='RC time constant'),
    I('τ = L/R',note='RL time constant'),
    I('x(t) = x(∞) + (x(0) − x(∞)) e^{−t/τ}',note='step response'))),
 S('AC power and phasors',
  G('Phasors',
    I('v(t) = V_{m} cos(ωt + φ)',note='sinusoid'),
    I('V = V_{m} ∠φ',note='phasor'),
    I('ω = 2πf',note='angular frequency'),
    I('V_{rms} = V_{peak}/√2',note='sine, V_peak = V_m'),
    I('V_{rms} = √((1/T) ∫_{T} v^{2} dt)',note='any periodic wave')),
  G('Impedance',
    I('Z_{R} = R',note='resistor'),
    I('Z_{L} = jωL',note='inductor'),
    I('Z_{C} = 1/(jωC)',note='capacitor'),
    I('V = I Z',note="Ohm's law for phasors"),
    I('Z = R + jX',note='impedance: resistance and reactance'),
    I('Z = R + j(ωL − 1/(ωC))',note='series RLC'),
    I('Y = 1/Z = G + jB',note='admittance'),
    I('X_{L} = ωL',note='inductive reactance'),
    I('|X_{C}| = 1/(ωC)',note='capacitive reactance')),
  G('Power',
    I('P = V_{rms} I_{rms} cos(θ)',note='real, W'),
    I('P = (1/2) V_{m} I_{m} cos(θ)',note='real, from peak values'),
    I('Q = V_{rms} I_{rms} sin(θ)',note='reactive, VAR'),
    I('S = V_{rms} I_{rms}^{*} = P + jQ',note='complex, VA'),
    I('PF = P/|S| = cos(θ)',note='power factor'),
    I('Z_{L} = Z_{th}^{*}',note='maximum power transfer: load Z_L'),
    I('P_{max} = V_{th}^{2}/(4R_{th})',note='V rms, matched load')),
  G('Three-phase',
    I('V_{L} = √3 V_{ph}',note='wye'),
    I('I_{L} = I_{ph}',note='wye'),
    I('V_{L} = V_{ph}',note='delta'),
    I('I_{L} = √3 I_{ph}',note='delta'),
    I('P = √3 V_{L} I_{L} cos(θ)',note='balanced')),
  G('Transformers (ideal)',
    I('V_{1}/V_{2} = N_{1}/N_{2}',note='turns ratio'),
    I('I_{1}/I_{2} = N_{2}/N_{1}'),
    I('Z_{in} = (N_{1}/N_{2})^{2} Z_{L}',note='seen from the primary'))),
 S('RLC and filters',
  G('RLC',
    I('ω_{0} = 1/√(LC)',note='resonance'),
    I('α = R/(2L)',note='neper frequency, series'),
    I('α = 1/(2RC)',note='neper frequency, parallel'),
    I('ζ = α/ω_{0}',note='damping ratio'),
    I('s = −α ± √(α^{2} − ω_{0}^{2})',note='natural roots'),
    I('ω_{d} = √(ω_{0}^{2} − α^{2})',note='damped frequency, underdamped'),
    I('Q = (1/R) √(L/C)',note='quality factor, series RLC'),
    I('Q = R √(C/L)',note='quality factor, parallel RLC'),
    I('BW = ω_{0}/Q',note='bandwidth, rad/s')),
  G('RLC natural and step response',
    I('s^{2} + (R/L) s + 1/(LC) = 0',note='series RLC characteristic equation'),
    I('v(t) = V_{f} + A_{1} e^{s_{1}t} + A_{2} e^{s_{2}t}',note='overdamped'),
    I('v(t) = V_{f} + (A_{1} + A_{2} t) e^{−αt}',note='critically damped, α = ω₀'),
    I('v(t) = V_{f} + e^{−αt} (B_{1} cos(ω_{d}t) + B_{2} sin(ω_{d}t))',note='underdamped')),
  G('Second-order step: overshoot and timing',
    I('H(s) = ω_{0}^{2}/(s^{2} + 2ζω_{0} s + ω_{0}^{2})'),
    I('%OS = 100 e^{−πζ/√(1 − ζ^{2})}',note='overshoot, 0 < ζ < 1'),
    I('t_{p} = π/ω_{d}',note='peak time'),
    I('t_{s} ≈ 4/(ζω_{0})',note='settling time, 2%'),
    I('t_{r} ≈ 1.8/ω_{0}',note='rise time 10-90%, near ζ = 0.5')),
  G('First-order filters',
    I('H = 1/(1 + jωRC)',note='RC low-pass'),
    I('H = jωRC/(1 + jωRC)',note='RC high-pass'),
    I('f_{c} = 1/(2πRC)',note='RC cutoff, −3 dB'),
    I('f_{c} = R/(2πL)',note='RL cutoff, −3 dB')),
  G('Bode',
    I('|H|_{dB} = 20 log_{10}|H|'),
    I('−3 dB and ±45°',note='at a first-order corner',copy='-3 dB'),
    I('20 dB/decade',note='slope per pole or zero'))),
 S('Op amps',
  G('Ideal rules',
    I('V_{+} = V_{−}',note='with negative feedback'),
    I('I_{+} = I_{−} = 0')),
  G('Circuits',
    I('V_{out}/V_{in} = −R_{f}/R_{in}',note='inverting amplifier gain'),
    I('V_{out}/V_{in} = 1 + R_{f}/R_{g}',note='non-inverting amplifier gain'),
    I('V_{out} = V_{in}',note='buffer, voltage follower'),
    I('V_{out} = −R_{f} (V_{1}/R_{1} + V_{2}/R_{2})',note='summing'),
    I('V_{out} = (R_{2}/R_{1})(V_{2} − V_{1})',note='difference, matched'),
    I('V_{out} = −(1/(RC)) ∫ V_{in} dt',note='integrator'),
    I('V_{out} = −RC dV_{in}/dt',note='differentiator')),
  G('Limits',
    I('GBW = A f',note='gain-bandwidth'),
    I('SR = 2πf V_{pk}',note='full-power slew rate'),
    I('CMRR = 20 log_{10}(A_{d}/A_{cm})',note='common-mode rejection'))),
 S('Diodes and transistors',
  G('Diode',
    I('I = I_{S} (e^{V/(nV_{T})} − 1)',note='Shockley'),
    I('V_{T} = kT/q',note='about 26 mV at 300 K'),
    I('r_{d} = nV_{T}/I_{D}',note='small signal'),
    I('V_{dc} = V_{m}/π',note='half-wave'),
    I('V_{dc} = 2V_{m}/π',note='full-wave'),
    I('V_{r} ≈ I/(fC)',note='ripple, f at the ripple rate')),
  G('BJT',
    I('I_{C} = β I_{B}',note='active'),
    I('I_{E} = I_{B} + I_{C}'),
    I('α = β/(β + 1)'),
    I('I_{C} = I_{S} e^{V_{BE}/V_{T}}'),
    I('g_{m} = I_{C}/V_{T}'),
    I('r_{π} = β/g_{m}'),
    I('r_{o} = V_{A}/I_{C}'),
    I('A_{v} = −g_{m} (R_{C} ∥ r_{o})',note='common emitter')),
  G('MOSFET',
    I('V_{ov} = V_{GS} − V_{th}',note='overdrive'),
    I('I_{D} = (1/2) μ_{n} C_{ox} (W/L) V_{ov}^{2}',note='saturation'),
    I('V_{DS} ≥ V_{ov}',note='to stay in saturation'),
    I('I_{D} = μ_{n} C_{ox} (W/L) (V_{ov} V_{DS} − V_{DS}^{2}/2)',note='triode'),
    I('g_{m} = 2I_{D}/V_{ov}'),
    I('r_{o} = 1/(λ I_{D})'),
    I('A_{v} = −g_{m} (R_{D} ∥ r_{o})',note='common source'),
    I('P = a C V^{2} f',note='CMOS switching, a activity'))),
 S('Digital and data conversion',
  G('Logic',
    I('¬(A · B) = ¬A + ¬B',note='De Morgan'),
    I('¬(A + B) = ¬A · ¬B',note='De Morgan'),
    I('A ⊕ B = A · ¬B + ¬A · B',note='XOR')),
  G('Numbers, n bits',
    I('0 to 2^{n} − 1',note='unsigned'),
    I('−2^{n−1} to 2^{n−1} − 1',note='two\'s complement')),
  G('Timing',
    I('T_{clk} ≥ t_{cq} + t_{logic} + t_{su}'),
    I('f_{max} = 1/T_{clk}')),
  G('Converters',
    I('LSB = V_{FS}/2^{n}'),
    I('V_{out} = V_{ref} D/2^{n}',note='DAC, code D'),
    I('SNR = 6.02n + 1.76 dB',note='ideal ADC'),
    I('f_{s} ≥ 2 f_{max}',note='Nyquist'),
    I('f_{a} = |f − k f_{s}|',note='alias, nearest k'))),
 S('Signals, noise and communication',
  G('Decibels',
    I('dB = 10 log_{10}(P_{2}/P_{1})',note='power'),
    I('dB = 20 log_{10}(V_{2}/V_{1})',note='voltage, same R'),
    I('dBm = 10 log_{10}(P/(1 mW))')),
  G('Noise',
    I('v_{n} = √(4 k T R B)',note='resistor, rms'),
    I('P_{n} = kTB',note='available power'),
    I('F = SNR_{in}/SNR_{out}',note='noise factor')),
  G('Links',
    I('C = B log_{2}(1 + S/N)',note='Shannon capacity'),
    I('P_{r} = P_{t} G_{t} G_{r} (λ/(4πd))^{2}',note='Friis'),
    I('c = f λ'))),
 S('Electromagnetics and transmission lines',
  G('Lines',
    I('Z_{0} = √(L′/C′)',note='lossless, per length'),
    I('v_{p} = 1/√(L′C′)',note='phase velocity'),
    I('β = 2π/λ',note='phase constant'),
    I('Γ = (Z_{L} − Z_{0})/(Z_{L} + Z_{0})',note='reflection'),
    I('VSWR = (1 + |Γ|)/(1 − |Γ|)',note='standing-wave ratio'),
    I('Z_{in} = Z_{0} (Z_{L} + jZ_{0} tan(βl))/(Z_{0} + jZ_{L} tan(βl))',note='lossless')),
  G('Components and fields',
    I('C = εA/d',note='parallel plates'),
    I('L = μN^{2}A/l',note='solenoid'),
    I('δ = √(2/(ωμσ))',note='skin depth'))),
 S('Motors and power electronics',
  G('Machines',
    I('V = E + I_{a} R_{a}',note='DC motor'),
    I('E = k ω',note='back EMF'),
    I('τ = k I_{a}',note='torque'),
    I('P = τ ω',note='shaft power'),
    I('n_{s} = 120f/p',note='synchronous rpm, p poles'),
    I('s = (n_{s} − n)/n_{s}',note='slip'),
    I('η = P_{out}/P_{in}',note='efficiency')),
  G('Converters, duty cycle D',
    I('V_{out} = D V_{in}',note='buck'),
    I('V_{out} = V_{in}/(1 − D)',note='boost'),
    I('V_{out} = −D V_{in}/(1 − D)',note='buck-boost'))),
 S('Control systems',
  G('Loops',
    I('T = G/(1 + GH)',note='closed loop'),
    I('1 + GH = 0',note='characteristic equation'),
    I('e_{ss} = 1/(1 + K_{p})',note='steady-state error, step, type 0'),
    I('K_{p} = lim_{s→0} G(s)',note='position error constant'),
    I('e_{ss} = 1/K_{v}',note='steady-state error, ramp, type 1'),
    I('K_{v} = lim_{s→0} s G(s)',note='velocity error constant')),
  G('PID',
    I('u = K_{p} e + K_{i} ∫ e dt + K_{d} de/dt',note='PID controller'),
    I('C(s) = K_{p} + K_{i}/s + K_{d} s',note='PID transfer function'))))

taylor=SH('taylor series','Taylor Series','taylor series: Maclaurin series, error bounds, convergence tests',
 S('General',
  G('About a',
    I('f(x) = Σ_{n=0}^{∞} f^{(n)}(a) (x − a)^{n}/n!',note='Taylor series'),
    I('f(x) = Σ_{n=0}^{∞} f^{(n)}(0) x^{n}/n!',note='Maclaurin series: a = 0'),
    I('P_{n}(x) = Σ_{k=0}^{n} f^{(k)}(a) (x − a)^{k}/k!',note='Taylor polynomial of degree n'),
    I('f(x) ≈ f(a) + f′(a)(x − a)',note='linear approximation, tangent line')),
  G('Error bounds',
    I('R_{n}(x) = f^{(n+1)}(c) (x − a)^{n+1}/(n + 1)!',note='Lagrange remainder, c between a and x'),
    I('|R_{n}(x)| ≤ M |x − a|^{n+1}/(n + 1)!',note="Taylor's inequality, |f⁽ⁿ⁺¹⁾| ≤ M between a and x"),
    I('|S − S_{n}| ≤ |a_{n+1}|',note='alternating series estimate: error under the first term left out')),
  G('Convergence',
    I('L = lim_{n→∞} |a_{n+1}/a_{n}|',note='ratio test: Σ a_{n} converges if L < 1, diverges if L > 1'),
    I('R = lim_{n→∞} |a_{n}/a_{n+1}|',note='radius of convergence of Σ a_{n} (x − a)^{n}'),
    I('L = lim_{n→∞} |a_{n}|^{1/n}',note='root test: converges if L < 1, diverges if L > 1'),
    I('Σ 1/n^{p}',note='p-series: converges if p > 1, diverges if p ≤ 1'),
    I('lim_{n→∞} a_{n} ≠ 0',note='divergence (nth-term) test: then Σ a_{n} diverges')),
  wide=True),
 S('Maclaurin series',
  G('Exponential and trig',
    I('e^{x} = Σ x^{n}/n! = 1 + x + x^{2}/2! + x^{3}/3! + …',note='all x'),
    I('sin x = Σ (−1)^{n} x^{2n+1}/(2n + 1)! = x − x^{3}/3! + x^{5}/5! − …',note='all x'),
    I('cos x = Σ (−1)^{n} x^{2n}/(2n)! = 1 − x^{2}/2! + x^{4}/4! − …',note='all x'),
    I('tan x = x + x^{3}/3 + 2x^{5}/15 + …',note='|x| < π/2'),
    I('tan^{−1} x = x − x^{3}/3 + x^{5}/5 − …',note='arctan, |x| ≤ 1'),
    I('sinh x = x + x^{3}/3! + x^{5}/5! + …',note='all x'),
    I('cosh x = 1 + x^{2}/2! + x^{4}/4! + …',note='all x')),
  G('Geometric, log and binomial',
    I('1/(1 − x) = Σ x^{n} = 1 + x + x^{2} + x^{3} + …',note='|x| < 1'),
    I('1/(1 + x) = 1 − x + x^{2} − x^{3} + …',note='|x| < 1'),
    I('ln(1 + x) = x − x^{2}/2 + x^{3}/3 − …',note='−1 < x ≤ 1'),
    I('ln(1 − x) = −x − x^{2}/2 − x^{3}/3 − …',note='−1 ≤ x < 1'),
    I('(1 + x)^{a} = 1 + ax + a(a − 1)x^{2}/2! + …',note='binomial, |x| < 1'),
    I('√(1 + x) = 1 + x/2 − x^{2}/8 + …',note='|x| ≤ 1')),
  G('Small x: first terms, for x near 0',
    I('sin x ≈ x',note='small angle, x in radians'),
    I('cos x ≈ 1 − x^{2}/2',note='small angle'),
    I('e^{x} ≈ 1 + x',note='small growth'),
    I('(1 + x)^{a} ≈ 1 + a x',note='binomial approximation'))))

vector=SH('vector calculus','Vector Calculus','vector calculus: gradient, divergence, curl, line and surface integrals, the integral theorems',
 S('Operators',
  G('In x, y, z',
    I('∇f = (∂f/∂x, ∂f/∂y, ∂f/∂z)',note='gradient'),
    I('∇ · F = ∂F_{x}/∂x + ∂F_{y}/∂y + ∂F_{z}/∂z',note='divergence'),
    I('∇ × F = (∂F_{z}/∂y − ∂F_{y}/∂z, ∂F_{x}/∂z − ∂F_{z}/∂x, ∂F_{y}/∂x − ∂F_{x}/∂y)',note='curl'),
    I('∇^{2} f = ∂^{2}f/∂x^{2} + ∂^{2}f/∂y^{2} + ∂^{2}f/∂z^{2}',note='Laplacian')),
  G('Using the gradient',
    I('D_{u} f = ∇f · u',note='directional derivative, u a unit vector'),
    I('z − z_{0} = f_{x}(x − x_{0}) + f_{y}(y − y_{0})',note='tangent plane to z = f(x, y)'),
    I('∇f = λ ∇g',note='Lagrange multipliers: extremes of f on g = k')),
  G('Identities',
    I('∇ × (∇f) = 0',note='curl of a gradient'),
    I('∇ · (∇ × F) = 0',note='divergence of a curl'),
    I('∇ · (f F) = f ∇ · F + F · ∇f',note='product rule'),
    I('∇ × (∇ × F) = ∇(∇ · F) − ∇^{2} F',note='curl of a curl')),
  wide=True),
 S('Integrals',
  G('Along curves and over surfaces',
    I('∫_{C} F · dr = ∫_{a}^{b} F(r(t)) · r′(t) dt',note='line integral, work'),
    I('∫_{C} f ds = ∫_{a}^{b} f(r(t)) |r′(t)| dt',note='scalar line integral: mass of a wire, arc length if f = 1'),
    I('ds = |r′(t)| dt',note='arc length element'),
    I('∬_{S} F · dS = ∬_{D} F · (r_{u} × r_{v}) du dv',note='flux integral'),
    I('∬_{S} F · dS = ∬_{D} F · (−g_{x}, −g_{y}, 1) dA',note='flux up through a graph z = g(x, y)'),
    I('A = ∬_{D} |r_{u} × r_{v}| du dv',note='surface area of a parametric surface')),
  G('Area and volume elements',
    I('dA = r dr dθ',note='polar'),
    I('dV = r dr dθ dz',note='cylindrical'),
    I('dV = ρ^{2} sin(φ) dρ dθ dφ',note='spherical, φ from the z axis'),
    I('x = ρ sin(φ) cos(θ),  y = ρ sin(φ) sin(θ),  z = ρ cos(φ)',note='spherical coordinates',copy='x = ρ sin(φ) cos(θ)'),
    I('dA = |∂(x, y)/∂(u, v)| du dv',note='Jacobian: change of variables')),
  wide=True),
 S('Theorems',
  G('The integral theorems',
    I('∫_{C} ∇f · dr = f(b) − f(a)',note='gradient theorem'),
    I('∮_{C} (P dx + Q dy) = ∬_{D} (∂Q/∂x − ∂P/∂y) dA',note="Green's"),
    I('A = (1/2) ∮_{C} (x dy − y dx)',note="area from Green's theorem, C counterclockwise"),
    I('∮_{C} F · dr = ∬_{S} (∇ × F) · dS',note="Stokes' theorem"),
    I('∯_{S} F · dS = ∭_{V} (∇ · F) dV',note='divergence theorem, Gauss')),
  G('Conservative fields',
    I('∇ × F = 0',note='then F = ∇f, on a simply connected region'),
    I('∂Q/∂x = ∂P/∂y',note='the 2D test for F = (P, Q)')),
  wide=True))

# search tags for fourier to vector calculus: words a student types that a row's note doesn't hold (acronyms, other
# names). Merged into TAGS, never replacing it.
TAGS=globals().get('TAGS',{})
for k,v in {
 # fourier, euler, complex numbers
 'a_{k} = (1/T) ∫_{T} x(t) e^{−jkω_{o}t} dt':'fourier series coefficients fundamental frequency',
 'a_{k} = (1/N) Σ_{n=<N>} x[n] e^{−jkω_{o}n}':'fourier series coefficients fundamental frequency DTFS',
 'x(t) = A_{0} + Σ_{k=1}^{∞} [A_{k} cos(kω_{o}t) + B_{k} sin(kω_{o}t)]':'trig trigonometric fourier series sine cosine form',
 'x(t) = A_{0} + Σ_{k=1}^{∞} C_{k} cos(kω_{o}t + φ_{k})':'amplitude phase form harmonics',
 'a cos(θ) + b sin(θ) = R cos(θ − φ)':'sum of sinusoids a cos b sin phase shift amplitude',
 'z = x + jy = r e^{jθ}':'polar form exponential form rectangular to polar',
 'r = |z| = √(x^{2} + y^{2})':'modulus absolute value magnitude',
 'θ = atan2(y, x)':'arg phase quadrant',
 '(a + jb)(c + jd) = (ac − bd) + j(ad + bc)':'multiply complex numbers rectangular',
 '(a + jb)/(c + jd) = ((ac + bd) + j(bc − ad))/(c^{2} + d^{2})':'divide complex numbers conjugate',
 'ln z = ln r + j(θ + 2πk)':'logarithm principal value',
 'z^{1/n} = r^{1/n} e^{j(θ + 2πk)/n}':'roots of unity',
 # transform tables
 'X(jω) = ∫_{−∞}^{∞} x(t) e^{−jωt} dt':'CTFT FT fourier transform definition',
 'X(e^{jω}) = Σ_{n=−∞}^{∞} x[n] e^{−jωn}':'DTFT definition',
 'X[k] = Σ_{n=0}^{N−1} x[n] e^{−j2πkn/N}':'DFT FFT',
 'f_{k} = k f_{s}/N':'frequency resolution FFT bin spacing',
 'X(z) = Σ_{n=−∞}^{∞} x[n] z^{−n}':'z transform definition',
 # unit circle, geometry
 '(x, y) = (cos θ, sin θ)':'ASTC CAST all students take calculus quadrant signs',
 'A = √(s(s − a)(s − b)(s − c))':'three sides half perimeter',
 'A = (1/2) a b sin(C)':'SAS included angle',
 '(n − 2) 180°':'polygon interior angles',
 '(x − h)^{2} + (y − k)^{2} = r^{2}':'equation of a circle',
 # statistics, linear algebra
 'Var(X + Y) = Var(X) + Var(Y) + 2σ_{XY}':'variance of a sum covariance independent',
 'x̄ ± t s/√n':'degrees of freedom t interval',
 'det A = a(ei − fh) − b(di − fg) + c(dh − eg)':'determinant 3x3 cofactor expansion',
 'x_{i} = det(A_{i})/det(A)':"cramer's rule",
 '(AB)_{ij} = Σ_{k} a_{ik} b_{kj}':'row times column product',
 'A^{T} = A':'symmetric matrix orthogonal eigenvectors',
 'Q^{T} Q = I':'orthogonal matrix det ±1 rotation',
 'cos(θ) = (a · b)/(|a| |b|)':'angle between vectors perpendicular orthogonal',
 'λ = T/2 ± √(T^{2}/4 − D)':'eigenvalues 2x2 trace determinant',
 'A = P D P^{−1}':'diagonalization eigendecomposition',
 'a × b = (a_{2}b_{3} − a_{3}b_{2}, a_{3}b_{1} − a_{1}b_{3}, a_{1}b_{2} − a_{2}b_{1})':'cross product vector product',
 # physics, circuits
 'v = v_{0} + a t':'kinematics suvat',
 'x = x_{0} + v_{0} t + (1/2) a t^{2}':'kinematics suvat',
 'v^{2} = v_{0}^{2} + 2a Δx':'kinematics suvat',
 'Δx = (1/2)(v_{0} + v) t':'kinematics suvat',
 'm_{1} v_{1} + m_{2} v_{2} = m_{1} v′_{1} + m_{2} v′_{2}':'conservation of momentum collision',
 'sin(θ_{c}) = n_{2}/n_{1}':'total internal reflection TIR',
 'f′ = f (v + v_{o})/(v − v_{s})':'approaching moving source observer',
 'CMRR = 20 log_{10}(A_{d}/A_{cm})':'common mode rejection ratio',
 '|H|_{dB} = 20 log_{10}|H|':'gain in decibels',
 # taylor, vector calculus
 '1/(1 − x) = Σ x^{n} = 1 + x + x^{2} + x^{3} + …':'geometric series',
 'R_{n}(x) = f^{(n+1)}(c) (x − a)^{n+1}/(n + 1)!':'taylor error bound',
 '∬_{S} F · dS = ∬_{D} F · (r_{u} × r_{v}) du dv':'surface integral parametrized surface',
 '∫_{C} ∇f · dr = f(b) − f(a)':'fundamental theorem for line integrals path independence',
 '∮_{C} (P dx + Q dy) = ∬_{D} (∂Q/∂x − ∂P/∂y) dA':"green's theorem counterclockwise circulation",
}.items():
    TAGS[k]=(TAGS.get(k,'')+' '+v).strip()
# round 3: Greek letters typed as words, other spellings, course phrases (fourier to vector calculus)
for k,v in {
 'a cos(θ) + b sin(θ) = R cos(θ − φ)':'harmonic addition phasor addition',
 'j^{2} = −1':'i squared i^2 imaginary unit',
 '|z_{1} + z_{2}| ≤ |z_{1}| + |z_{2}|':'modulus of a sum',
 'r = |z| = √(x^{2} + y^{2})':'abs',
 'f_{s} ≥ 2 f_{max}':'sampling theorem nyquist rate shannon sampling',
 'f(t) = (1/2πj) ∫_{σ−j∞}^{σ+j∞} F(s) e^{st} ds':'inverse laplace transform bromwich partial fractions',
 'A = (1/4) n s^{2} cot(π/n)':'hexagon pentagon octagon polygon area',
 'c^{2} = a^{2} + b^{2} − 2ab cos(C)':'cosine rule',
 'a/sin(A) = b/sin(B) = c/sin(C)':'sine rule',
 'x̄ = (1/n) Σ x_{i}':'mu arithmetic mean xbar',
 's = √(Σ (x_{i} − x̄)^{2}/(n − 1))':'sigma stdev std dev spread',
 'σ^{2} = Σ (x_{i} − μ)^{2}/N':'sigma squared',
 'b_{1} = r s_{y}/s_{x}':'linear regression line of best fit',
 'z = (x̄ − μ_{0})/(σ/√n)':'p value significance test statistic',
 't = (x̄ − μ_{0})/(s/√n)':'p value student t',
 'within 1σ: 68.27%':'empirical rule 68 95 99.7 sigma',
 'within 2σ: 95.45%':'empirical rule 68 95 99.7 sigma',
 'within 3σ: 99.73%':'empirical rule 68 95 99.7 sigma',
 'A v = λ v':'lambda eigen eigenpair',
 'det(A − λI) = 0':'lambda characteristic polynomial eigen',
 'A = P D P^{−1}':'eigen decomposition diagonalize diagonalise spectral',
 'a · b = Σ a_{i} b_{i} = |a| |b| cos(θ)':'scalar product inner product',
 'f(x) ≈ f(a) + f′(a)(x − a)':'linearization linearisation first order approximation',
 'e^{x} = Σ x^{n}/n! = 1 + x + x^{2}/2! + x^{3}/3! + …':'exp series exponential series',
 '∇f = (∂f/∂x, ∂f/∂y, ∂f/∂z)':'nabla del grad',
 '∇ · F = ∂F_{x}/∂x + ∂F_{y}/∂y + ∂F_{z}/∂z':'nabla del dot div',
 '∇ × F = (∂F_{z}/∂y − ∂F_{y}/∂z, ∂F_{x}/∂z − ∂F_{z}/∂x, ∂F_{y}/∂x − ∂F_{x}/∂y)':'nabla del cross rot',
 '∇^{2} f = ∂^{2}f/∂x^{2} + ∂^{2}f/∂y^{2} + ∂^{2}f/∂z^{2}':'del squared nabla squared laplace operator',
 'dA = r dr dθ':'jacobian change of variables polar coordinates',
 'dV = r dr dθ dz':'jacobian change of variables',
 'dV = ρ^{2} sin(φ) dρ dθ dφ':'jacobian change of variables',
 '∬_{S} F · dS = ∬_{D} F · (r_{u} × r_{v}) du dv':'flux through a surface',
 'ω_{0} = 1/√(LC)':'omega n omega 0 natural frequency undamped',
 'ζ = α/ω_{0}':'zeta damping ratio rlc',
 'H(s) = ω_{0}^{2}/(s^{2} + 2ζω_{0} s + ω_{0}^{2})':'second order system standard form zeta omega n',
 '%OS = 100 e^{−πζ/√(1 − ζ^{2})}':'percent overshoot zeta',
 'V_{rms} = V_{peak}/√2':'rms root mean square',
 'V_{rms} = √((1/T) ∫_{T} v^{2} dt)':'rms root mean square',
 'S = V_{rms} I_{rms}^{*} = P + jQ':'apparent power volt amperes',
 'PF = P/|S| = cos(θ)':'pf lagging leading',
 'f_{c} = 1/(2πRC)':'corner frequency break frequency half power',
 'f_{c} = R/(2πL)':'corner frequency break frequency half power',
 '−3 dB and ±45°':'corner frequency half power',
 'V_{out}/V_{in} = 1 + R_{f}/R_{g}':'noninverting amplifier',
 'I_{C} = β I_{B}':'beta hfe current gain',
 'R_{1} = R_{b} R_{c}/(R_{a} + R_{b} + R_{c})':'star delta pi tee y delta',
 'R_{a} = (R_{1}R_{2} + R_{2}R_{3} + R_{3}R_{1})/R_{1}':'star delta pi tee y delta',
 'Γ = (Z_{L} − Z_{0})/(Z_{L} + Z_{0})':'gamma reflection coefficient',
 'V_{L} = √3 V_{ph}':'line voltage phase voltage star',
 'g_{m} = I_{C}/V_{T}':'transconductance',
 'g_{m} = 2I_{D}/V_{ov}':'transconductance',
}.items():
    TAGS[k]=(TAGS.get(k,'')+' '+v).strip()
# critic round 1 (euler, complex numbers, taylor, vector calculus, precalculus, differential equations, numerical
# methods): one-word spellings, misspellings and course phrases that found nothing
for k,v in {
 '(cos θ + j sin θ)^{n} = cos(nθ) + j sin(nθ)':'demoivre moivre',
 'z^{n} = r^{n} e^{jnθ}':'demoivre powers of a complex number',
 '∇ × F = 0':'irrotational curl free curl-free potential function',
 '∇f = (∂f/∂x, ∂f/∂y, ∂f/∂z)':'gradiant',
 'A = L U':'gaussian elimination gauss forward substitution back substitution',
 'y = c_{1} e^{r_{1}t} + c_{2} e^{r_{2}t}':'overdamped two real roots',
 'y = (c_{1} + c_{2} t) e^{rt}':'critically damped critical damping double root',
 'y = e^{αt} (c_{1} cos(βt) + c_{2} sin(βt))':'underdamped oscillating complex roots',
}.items():
    TAGS[k]=(TAGS.get(k,'')+' '+v).strip()
# critic round 4, an exam walkthrough (precalculus to complex numbers): the words a student types for the rows added
# then, and for the rows a timed search missed
for k,v in {
 'f(x) = p(x)/q(x)':'horizontal asymptote vertical asymptote slant oblique hole rational',
 '|x − a| < b ⇔ a − b < x < a + b':'abs inequality modulus inequality',
 'sin θ = opp/hyp,  cos θ = adj/hyp,  tan θ = opp/adj':'sohcahtoa right triangle trig ratios opposite adjacent hypotenuse',
 'y = A sin(B(x − C)) + D':'amplitude period phase shift midline vertical shift sine graph',
 'dQ/dt = c_{in} r_{in} − (Q/V) r_{out}':'brine salt tank rate in rate out mixture',
 'y_{2} = y_{1} ∫ e^{−∫ p dt}/y_{1}^{2} dt':'second solution second linearly independent',
 'ℒ{y′} = s Y − y(0)':'laplace of derivative laplace transform initial value problem ivp',
 'ℒ{y′′} = s^{2} Y − s y(0) − y′(0)':'laplace of second derivative ivp',
 'ℒ{u(t − a) f(t − a)} = e^{−as} F(s)':'unit step function heaviside second shifting theorem t-shift piecewise forcing',
 'ℒ{δ(t − a)} = e^{−as}':'dirac delta impulse function',
 'n ≥ log_{2}((b − a)/ε)':'how many iterations number of steps bisection tolerance',
 'x_{k+1} = x_{k} − J^{−1}(x_{k}) F(x_{k})':'newton raphson system nonlinear system jacobian multivariable newton',
 'f[x_{0}, x_{1}] = (f(x_{1}) − f(x_{0}))/(x_{1} − x_{0})':'divided differences table',
 'p(x) = f[x_{0}] + f[x_{0}, x_{1}](x − x_{0}) + f[x_{0}, x_{1}, x_{2}](x − x_{0})(x − x_{1}) + …':'newton interpolation newton form newton polynomial',
 '∫ f dx ≈ (3h/8)(f_{0} + 3f_{1} + 3f_{2} + f_{3})':'simpson 3/8 simpsons three eighths',
 '∫_{−1}^{1} f dx ≈ f(−1/√3) + f(1/√3)':'gauss quadrature gaussian quadrature gauss points',
 'E_{S} = −(b − a) h^{4} f^{(4)}(ξ)/180':'simpson error bound simpsons error',
 'x_{i}^{(k+1)} = (b_{i} − Σ_{j<i} a_{ij} x_{j}^{(k+1)} − Σ_{j>i} a_{ij} x_{j}^{(k)})/a_{ii}':'gauss seidel seidel',
 'P_{n}(x) = Σ_{k=0}^{n} f^{(k)}(a) (x − a)^{k}/k!':'nth degree taylor polynomial',
 '|R_{n}(x)| ≤ M |x − a|^{n+1}/(n + 1)!':'taylor error bound remainder estimate',
 '|S − S_{n}| ≤ |a_{n+1}|':'alternating series remainder alternating series error bound',
 'Σ 1/n^{p}':'p series harmonic series',
 'lim_{n→∞} a_{n} ≠ 0':'test for divergence nth term test',
 'R = lim_{n→∞} |a_{n}/a_{n+1}|':'interval of convergence',
 'L = lim_{n→∞} |a_{n}|^{1/n}':'nth root test',
 'z − z_{0} = f_{x}(x − x_{0}) + f_{y}(y − y_{0})':'linear approximation linearization two variables',
 '∇f = λ ∇g':'lagrange multiplier constrained optimization lambda',
 'D_{u} f = ∇f · u':'maximum rate of change steepest ascent',
 '∫_{C} f ds = ∫_{a}^{b} f(r(t)) |r′(t)| dt':'line integral ds mass of a wire scalar field',
 'A = ∬_{D} |r_{u} × r_{v}| du dv':'surface area',
 '∬_{S} F · dS = ∬_{D} F · (−g_{x}, −g_{y}, 1) dA':'flux through a graph surface integral',
 'A = (1/2) ∮_{C} (x dy − y dx)':"greens theorem area",
 'x = ρ sin(φ) cos(θ),  y = ρ sin(φ) sin(θ),  z = ρ cos(φ)':'rho phi theta spherical to rectangular',
 'dA = |∂(x, y)/∂(u, v)| du dv':'jacobian determinant change of variables substitution',
 'cos(2A) = 2 cos^{2}(A) − 1 = 1 − 2 sin^{2}(A)':'cos 2a 2cos^2 - 1 1 - 2sin^2',
 'cos(A) + cos(B) = 2 cos((A + B)/2) cos((A − B)/2)':'beats beat frequency cos a + cos b',
 'cosh^{2}(x) − sinh^{2}(x) = 1':'cosh^2 - sinh^2 hyperbolic identity',
 'cos(θ) = (1/2) e^{jθ} + (1/2) e^{−jθ}':'cos in terms of exponentials cosine exponential form',
 'sin(θ) = (1/2j) e^{jθ} − (1/2j) e^{−jθ}':'sin in terms of exponentials sine exponential form',
 'j^{3} = −j,  j^{4} = 1':'powers of i i^3 i^4 j^3 j^4',
 't_{d} = ln(2)/r':'rule of 70',
}.items():
    TAGS[k]=(TAGS.get(k,'')+' '+v).strip()
# critic round 5 (precalculus to complex numbers): "solve ode laplace" asks for the method, so the differential
# equations sheet's own transform rows lead it; the Laplace table's derivative row follows (and leads on its sheet)
for k,v in {
 'ℒ{y′} = s Y − y(0)':'solve ode laplace differential equation initial conditions',
 'ℒ{y′′} = s^{2} Y − s y(0) − y′(0)':'solve ode laplace differential equation initial conditions',
 'c_{c} = 2√(k m)':'critically damped ζ = 1 zeta',
 'y_{n+1} = y_{n} + h f(t_{n}, y_{n})':'ode solver ode solvers numerical ode',
}.items():
    TAGS[k]=(TAGS.get(k,'')+' '+v).strip()
# critic round 2 (precalculus, differential equations, numerical methods, taylor, vector calculus, euler, complex
# numbers): formulas typed without spaces, other names and course phrases that missed
for k,v in {
 'y = m x + b':'y=mx+b y=mx+c',
 'Δ = b^{2} − 4ac':'b^2-4ac b2-4ac',
 'a^{1/n} = ^{n}√a':'rational exponent',
 'log_{b}(x) = ln(x)/ln(b)':'change of base formula',
 'M dx + N dy = 0':'exact differential equation exact ode',
 'y_{n+1} = y_{n} + (k_{1} + 2k_{2} + 2k_{3} + k_{4})/6':'rk4 runge-kutta classical fourth order method',
 'x_{k+1} = A x_{k}/‖A x_{k}‖':'power iteration',
 '|e_{n+1}| ≈ C |e_{n}|^{p}':'rate of convergence convergence rate linear quadratic',
 'sin x = Σ (−1)^{n} x^{2n+1}/(2n + 1)! = x − x^{3}/3! + x^{5}/5! − …':'sine series sin series',
 'cos x = Σ (−1)^{n} x^{2n}/(2n)! = 1 − x^{2}/2! + x^{4}/4! − …':'cosine series cos series',
 '1/(1 − x) = Σ x^{n} = 1 + x + x^{2} + x^{3} + …':'1/(1-x)',
 'ln(1 + x) = x − x^{2}/2 + x^{3}/3 − …':'ln(1+x) log(1+x) log series',
 'ln(1 − x) = −x − x^{2}/2 − x^{3}/3 − …':'ln(1-x) log(1-x)',
 '√(1 + x) = 1 + x/2 − x^{2}/8 + …':'sqrt(1+x) square root series',
 'e^{jθ} = cos(θ) + j sin(θ)':'e^ix e^(ix) e^(i theta) e^(itheta) e^(jtheta) cis',
 'e^{jπ} + 1 = 0':'e^(i pi) e^(ipi) e^ipi',
 'cos(jx) = cosh(x)':'cos(ix) cos(jx)',
 'sin(jx) = j sinh(x)':'sin(ix) sin(jx)',
 'cos(A ± B) = cos(A) cos(B) ∓ sin(A) sin(B)':'sum of angles compound angle',
 'sin(A ± B) = sin(A) cos(B) ± cos(A) sin(B)':'sum of angles compound angle',
 'z = x + jy = r e^{jθ}':'argand diagram complex plane cis',
 'j = i':'i or j engineering notation',
 'z z^{*} = |z|^{2}':'z times its conjugate',
 'a^{m}a^{n} = a^{m+n}':'exponent rules',
 'y = y_{0} e^{kt}':'exponential decay',
 'a_{n} = a_{1} r^{n−1}':'common ratio',
 'S_{n} = a_{1}(1 − r^{n})/(1 − r)':'common ratio',
 'y = (c_{1} + c_{2} t) e^{rt}':'repeated root',
 'u = X(x) T(t)':'pde product solution',
 'sin(θ) = (1/2j) e^{jθ} − (1/2j) e^{−jθ}':'sine from exponentials exponential form of sine',
 'cos(θ) = (1/2) e^{jθ} + (1/2) e^{−jθ}':'cosine from exponentials exponential form of cosine',
 'sinh(x) = (e^{x} − e^{−x})/2':'hyperbolic sine',
 'cosh(x) = (e^{x} + e^{−x})/2':'hyperbolic cosine',
 'a cos(θ) + b sin(θ) = R cos(θ − φ)':'amplitude phase form',
 '∇ × (∇ × F) = ∇(∇ · F) − ∇^{2} F':'curl curl vector laplacian',
 'cos(2A) = cos^{2}(A) − sin^{2}(A)':'cos double angle cosine',
}.items():
    TAGS[k]=(TAGS.get(k,'')+' '+v).strip()
# critic round 2 (fourier, CT and DT transform tables, laplace, z transform, linear algebra): words a row is found by,
# keyed by its lead cells and show joined by ' ‖ ' as an explanation is (a table's frequency side repeats across sheets)
ROW_TAGS={
 # fourier
 'CT ‖ a_{k} = (1/T) ∫_{T} x(t) e^{−jkω_{o}t} dt':'analysis equation',
 'DT ‖ a_{k} = (1/N) Σ_{n=<N>} x[n] e^{−jkω_{o}n}':'analysis equation',
 'CT ‖ x(t) = Σ_{k=−∞}^{∞} a_{k} e^{jkω_{o}t}':'synthesis equation exponential complex fourier series',
 'DT ‖ x[n] = Σ_{k=<N>} a_{k} e^{jkω_{o}n}':'synthesis equation exponential complex fourier series DTFS',
 'a_{k} = (A_{k} − jB_{k})/2':'convert trig to complex exponential fourier coefficients',
 'dx/dt ‖ jkω_{o} a_{k}':'derivative',
 # CT transforms
 'Synthesis ‖ x(t) = (1/2π) ∫_{−∞}^{∞} X(jω) e^{jωt} dω':'inverse CTFT inverse fourier transform IFT',
 'u(t) ‖ 1/(jω) + πδ(ω)':'heaviside unit step',
 'sgn(t) ‖ 2/(jω)':'signum sgn',
 'e^{−a|t|}, a > 0 ‖ 2a/(a^{2} + ω^{2})':'double sided exponential laplacian lorentzian',
 'e^{jω_{o}t} ‖ 2πδ(ω − ω_{o})':'complex exponential phasor',
 '1 for |t| < T_{1}, 0 else ‖ 2 sin(ωT_{1})/ω':'box function boxcar rect rectangle',
 'sin(Wt)/(πt) ‖ 1 for |ω| < W, 0 else':'sinc function ideal lowpass filter brick wall',
 '1 − |t|/T for |t| < T, 0 else ‖ T sin^{2}(ωT/2)/(ωT/2)^{2}':'tri function triangular',
 'Σ_{n=−∞}^{∞} δ(t − nT) ‖ (2π/T) Σ_{k=−∞}^{∞} δ(ω − 2πk/T)':'dirac comb shah sampling function',
 'x(t − t_{0}) ‖ e^{−jωt_{0}} X(jω)':'delay',
 'd^{n}x(t)/dt^{n} ‖ (jω)^{n} X(jω)':'derivative',
 '∫ |x(t)|^{2} dt ‖ (1/2π) ∫ |X(jω)|^{2} dω':'rayleigh energy theorem energy spectral density',
 # DT transforms
 'δ[n] ‖ 1':'kronecker delta discrete impulse',
 'δ[n − n_{o}] ‖ e^{−jωn_{o}}':'shifted impulse discrete impulse',
 'δ(t − t_{0}) ‖ e^{−jωt_{0}}':'shifted delta delayed impulse',
 'e^{jω_{o}n} ‖ 2πδ(ω − ω_{o})':'complex exponential phasor',
 'u[n] ‖ 1/(1 − e^{−jω}) + πδ(ω)':'heaviside unit step sequence',
 'a^{n} u[n], |a| < 1 ‖ 1/(1 − ae^{−jω})':'exponential sequence',
 'cos(ω_{o}n) ‖ πδ(ω − ω_{o}) + πδ(ω + ω_{o})':'cosine',
 'sin(ω_{o}n) ‖ (π/j)[δ(ω − ω_{o}) − δ(ω + ω_{o})]':'sine',
 '1 for |n| ≤ N_{1}, 0 else ‖ sin(ω(N_{1} + 1/2))/sin(ω/2)':'dirichlet kernel box function',
 'Σ_{k=−∞}^{n} x[k] ‖ X(e^{jω})/(1 − e^{−jω}) + πX(e^{j0}) δ(ω)':'running sum cumulative sum',
 'x[n] y[n] ‖ (1/2π) ∫_{2π} X(e^{jθ}) Y(e^{j(ω − θ)}) dθ':'windowing window periodic convolution',
 'Analysis ‖ X[k] = Σ_{n=0}^{N−1} x[n] e^{−j2πkn/N}':'fast fourier transform',
 'Synthesis ‖ x[n] = (1/N) Σ_{k=0}^{N−1} X[k] e^{j2πkn/N}':'IDFT IFFT inverse FFT',
 # laplace
 'F(s) = ∫_{0}^{∞} f(t) e^{−st} dt':'laplace transform unilateral',
 'u(t) ‖ 1/s':'heaviside step',
 'e^{−at} ‖ 1/(s + a)':'exponential',
 '1 − e^{−at} ‖ a/(s(s + a))':'step response first order',
 't e^{−at} ‖ 1/(s + a)^{2}':'double pole repeated root',
 't^{n} e^{−at} ‖ n!/(s + a)^{n+1}':'higher order pole repeated root',
 '(e^{−at} − e^{−bt})/(b − a) ‖ 1/((s + a)(s + b))':'partial fractions',
 't sin(ωt) ‖ 2ωs/(s^{2} + ω^{2})^{2}':'resonance',
 'e^{−at} sin(ωt) ‖ ω/((s + a)^{2} + ω^{2})':'damped sinusoid underdamped',
 'e^{−at} cos(ωt) ‖ (s + a)/((s + a)^{2} + ω^{2})':'damped sinusoid underdamped',
 'f(t − a) u(t − a), a ≥ 0 ‖ e^{−as} F(s)':'second shifting theorem t-shifting t shift heaviside',
 'e^{−at} f(t) ‖ F(s + a)':'first shifting theorem s-shifting s shift',
 'df/dt ‖ s F(s) − f(0)':'laplace of derivative',
 '∫_{0}^{t} f(τ) dτ ‖ F(s)/s':'laplace of integral',
 't f(t) ‖ −dF/ds':'derivative of transform frequency differentiation',
 'f(0⁺) = lim_{s→∞} s F(s)':'IVT',
 'f(∞) = lim_{s→0} s F(s)':'FVT steady state',
 # z transform
 'X(z) = Σ_{n=−∞}^{∞} x[n] z^{−n}':'z-transform ztransform bilateral',
 'z = e^{sT}':'pole mapping',
 'a^{n} u[n] ‖ 1/(1 − az^{−1})':'geometric sequence right sided',
 '−a^{n} u[−n − 1] ‖ 1/(1 − az^{−1})':'left sided anti-causal',
 'cos(ω_{o}n) u[n] ‖ (1 − cos(ω_{o}) z^{−1})/(1 − 2cos(ω_{o}) z^{−1} + z^{−2})':'cosine',
 'sin(ω_{o}n) u[n] ‖ sin(ω_{o}) z^{−1}/(1 − 2cos(ω_{o}) z^{−1} + z^{−2})':'sine',
 'x[n − n_{o}] ‖ z^{−n_{o}} X(z)':'delay',
 'n x[n] ‖ −z dX/dz':'differentiation in z',
 # linear algebra
 'A A^{−1} = A^{−1} A = I':'invertible nonsingular',
 'x = A^{−1} b':'system of equations linear system',
 'R_{i} ← R_{i} − (a_{ik}/a_{kk}) R_{k}':'gaussian elimination gauss row reduction row reduce row echelon form rref pivot back substitution',
 'x_{i} = det(A_{i})/det(A)':'kramer',
 'det A = a(ei − fh) − b(di − fg) + c(dh − eg)':'laplace expansion minors',
 'A^{T} = A':'symetric',
 'Q^{T} Q = I':'orthonormal',
 'u = a/|a|':'normalize normalise',
}
for sh in (fourier,ct,dt,laplace,ztr,linalg):
    for sec in sh['sections']:
        for g in sec['groups']:
            for it in g['items']:
                extra=ROW_TAGS.pop(' ‖ '.join(it.get('lead',[])+[it['show']]),None)
                if extra: it['tags']=(it.get('tags','')+' '+extra).strip()
assert not ROW_TAGS,ROW_TAGS
# every laplace pair's ROC
for it in laplace['sections'][1]['groups'][0]['items'][1:]: it['tags']=(it.get('tags','')+' ROC region of convergence').strip()
# a row's own tags, where its show isn't its own: every z pair's ROC, every unit circle angle
for it in ztr['sections'][1]['groups'][0]['items'][1:]: it['tags']=(it.get('tags','')+' ROC region of convergence').strip()
for it in circle['sections'][1]['groups'][0]['items'][1:]:
    d=it['lead'][0].rstrip('°'); it['tags']=f'sin {d} cos {d} tan {d} degrees'
# a table's impulse, by its time cell
for sh in (ct,dt,laplace,ztr):
    for it in sh['sections'][1]['groups'][0]['items']:
        if it.get('lead',[''])[0] in ('δ(t)','δ[n]'): it['tags']=(it.get('tags','')+' unit impulse dirac delta function unit sample').strip()
# critic round 4 (an exam walkthrough of fourier, the CT and DT tables, laplace, z transform, unit circle, linear
# algebra): the phrase a student searches for, keyed as ROW_TAGS above
R4_TAGS={
 'B_{k} = 0':'even function',
 'A_{0} = A_{k} = 0':'odd function',
 'x(t) = (4A/π) Σ_{k=1,3,5,…} sin(kω_{o}t)/k':'gibbs phenomenon overshoot',
 'a_{k} = sin(kω_{o}T_{1})/(kπ)':'duty cycle rectangular pulse train periodic square wave',
 'LTI ‖ b_{k} = a_{k} H(jkω_{o})':'filtered square wave periodic input to an lti system filter harmonics',
 'Sinusoid ‖ y(t) = |H(jω_{o})| cos(ω_{o}t + ∠H(jω_{o}))':'lti response to a cosine sinusoidal input eigenfunction',
 'x(t) cos(ω_{o}t) ‖ (1/2)[X(j(ω − ω_{o})) + X(j(ω + ω_{o}))]':'amplitude modulation multiply by a cosine carrier mixing',
 'x(t) Σ_{n} δ(t − nT) ‖ (1/T) Σ_{k} X(j(ω − 2πk/T))':'sampling spectrum aliasing sampled signal',
 'x[n] cos(ω_{o}n) ‖ (1/2)[X(e^{j(ω − ω_{o})}) + X(e^{j(ω + ω_{o})})]':'dtft windowed sequence windowed sinusoid',
 '1 for 0 ≤ n ≤ N − 1, 0 else ‖ e^{−jω(N−1)/2} sin(ωN/2)/sin(ω/2)':'spectral leakage boxcar',
 'Response ‖ H(e^{jω}) = (Σ b_{k} e^{−jωk})/(Σ a_{k} e^{−jωk})':'fir filter iir filter',
 'Sampling ‖ ω = ΩT = 2πf/f_{s}':'digital frequency normalized frequency',
 'x[n/k] if k divides n, 0 else ‖ X(e^{jkω})':'zero insertion interpolation expander',
 'F(s) = A_{1}/(s − p_{1}) + A_{2}/(s − p_{2}) + …':'partial fraction expansion inverse laplace',
 'A_{r−k} = (1/k!) [d^{k}/ds^{k} ((s − p)^{r} F(s))]_{s = p}':'repeated root',
 'df/dt ‖ s F(s) − f(0)':'solve ode laplace differential equation initial conditions',
 '|z| > max|p_{i}|':'roc causal right sided',
 '|z| < min|p_{i}|':'anticausal anti-causal left sided',
 'r_{1} < |z| < r_{2}':'annulus possible rocs two sided',
 'x[n − 1] ‖ z^{−1} X(z) + x[−1]':'unilateral z transform one sided initial conditions',
 'u[n] − u[n − N] ‖ (1 − z^{−N})/(1 − z^{−1})':'finite length sequence moving average boxcar',
 'θ_{ref} = 180° − θ,  θ − 180°,  360° − θ':'related acute angle astc all students take calculus quadrant signs',
 'sin(x + 2π) = sin x':'coterminal angle periodic',
 'arcsin x ∈ [−π/2, π/2]':'principal value sin^-1 inverse sine asin',
 'arccos x ∈ [0, π]':'principal value cos^-1 inverse cosine acos',
 'arctan x ∈ (−π/2, π/2)':'principal value tan^-1 inverse tangent atan',
 'sin(−x) = −sin x':'negative angle odd function',
 'cos(−x) = cos x':'negative angle even function',
 'A^{−1} = C^{T}/det(A)':'inverse 3x3 inverse of a 3 by 3 adjoint adjugate cofactor',
 'rank(A) = rank([A | b])':'consistent system augmented matrix no solution infinitely many solutions',
 'u_{k} = v_{k} − Σ_{j<k} ((v_{k} · u_{j})/(u_{j} · u_{j})) u_{j}':'orthonormal basis orthogonalize gram schmidt',
 'A = Q R':'qr decomposition',
}
for sh in (fourier,ct,dt,laplace,ztr,circle,linalg):
    for sec in sh['sections']:
        for g in sec['groups']:
            for it in g['items']:
                extra=R4_TAGS.pop(' ‖ '.join(it.get('lead',[])+[it['show']]),None)
                if extra: it['tags']=(it.get('tags','')+' '+extra).strip()
assert not R4_TAGS,R4_TAGS

# ---- the dynamics sheet's little pictures: each body drawn in Typst beside its volume, with its axes, so the
# formulas under it say which axis is which. black is the ink colour (it follows light and dark); the shading is a
# see-through grey, which reads in both
GREY='rgb(128, 128, 128, 70)'
def proj(x,y,z,k=0.42):
    "oblique: z comes toward the reader, down and to the left"
    return (x-k*z*0.866, y-k*z*0.5)
def ell(cx,cy,rx,ry,a0=0,a1=360,n=48,tilt=0):
    out=[]
    for i in range(n+1):
        t=math.radians(a0+(a1-a0)*i/n)
        x,y=rx*math.cos(t),ry*math.sin(t)
        c,s=math.cos(math.radians(tilt)),math.sin(math.radians(tilt))
        out.append((cx+x*c-y*s,cy+x*s+y*c))
    return out
def bez(S,C,E,n=16):
    "a quadratic curve from S to E, pulled toward C"
    return [((1-t)**2*S[0]+2*(1-t)*t*C[0]+t*t*E[0],(1-t)**2*S[1]+2*(1-t)*t*C[1]+t*t*E[1]) for t in [i/n for i in range(n+1)]]
def bvec(l):
    "the book's unit vector: a bold upright b, hatted, its bold upright letter beside it"
    return 'hat(bold(upright(b)))_(bold(upright(%s)))'%l
class Pic:
    "a picture in points, y up; its box fits what's drawn, whatever the origin; the dynamics bodies drawn at 1.6"
    SCALE=1.6
    def __init__(s,*_,**__): s.ops=[]; s.box=[]; s.curl=0
    def at(s,p): return (p[0]*s.SCALE,p[1]*s.SCALE)
    def grow(s,p,dx=0,dy=0):
        x,y=s.at(p); s.box+=[(x-dx,y-dy),(x+dx,y+dy)]
    def stroke(s,w,dash): return '(paint: black, thickness: %gpt%s)'%(w,', dash: "%s"'%dash if dash else '')
    def path(s,pts,closed=False,fill=None,w=0.7,dash=None,stroke=True,holes=()):
        for q in list(pts)+[q for h in holes for q in h]: s.grow(q,w,w)
        opts=(['fill: '+fill] if fill else [])+(['fill-rule: "even-odd"'] if holes else [])+['stroke: '+(s.stroke(w,dash) if stroke else 'none')]
        def draw(T):
            parts=['curve.move(%s)'%T(pts[0])]+['curve.line(%s)'%T(q) for q in pts[1:]]
            if closed: parts.append('curve.close()')
            for h in holes: parts+=['curve.move(%s)'%T(h[0])]+['curve.line(%s)'%T(q) for q in h[1:]]+['curve.close()']
            return 'place(top + left, curve(%s, %s))'%(', '.join(opts),', '.join(parts))
        s.ops.append(draw)
    def line(s,a,b,w=0.7,dash=None): s.path([a,b],w=w,dash=dash)
    def head(s,p,u,head=3.2):
        "a filled arrowhead with its point at p, pointing along u (unit), head long in points"
        head/=s.SCALE; ux,uy=u
        s.path([p,(p[0]-ux*head-uy*head*.42,p[1]-uy*head+ux*head*.42),(p[0]-ux*head+uy*head*.42,p[1]-uy*head-ux*head*.42)],closed=True,fill='black',stroke=False)
    def arrow(s,a,b,w=0.7,head=3.2,both=False):
        h=head/s.SCALE
        dx,dy=b[0]-a[0],b[1]-a[1]; L=math.hypot(dx,dy); ux,uy=dx/L,dy/L
        a2=(a[0]+ux*h*.8,a[1]+uy*h*.8) if both else a
        s.line(a2,(b[0]-ux*h*.8,b[1]-uy*h*.8),w)
        s.head(b,(ux,uy),head)
        if both: s.head(a,(-ux,-uy),head)
    def curved(s,pts,w=0.7,head=3.2):
        "a curve through pts with an arrowhead at its last point"
        a,b=pts[-3],pts[-1]; L=math.hypot(b[0]-a[0],b[1]-a[1]); u=((b[0]-a[0])/L,(b[1]-a[1])/L); h=head/s.SCALE*.8
        s.path(pts[:-1]+[(b[0]-u[0]*h,b[1]-u[1]*h)],w=w); s.head(b,u,head)
    def angle(s,c,r,a0,a1,m=None,at=None,w=0.45,head=2.4):
        "an angle's arc about c from a0 to a1 degrees, an arrowhead at each end, its name at radius at"
        pts=ell(c[0],c[1],r,r,a0,a1,24); s.curved(pts,w,head); s.curved(pts[::-1],w,head)
        if m: q=ell(c[0],c[1],at or r*0.62,at or r*0.62,(a0+a1)/2,(a0+a1)/2,1)[0]; s.label(q,m,6.5)
    def label(s,p,m,size=7,w=None,h=None):
        # roughly the label's own size, so the box holds it
        wide=w if w else len(re.sub(r'[a-z]+\(|\)|_|"','',m))*size*0.3+size*0.3
        s.grow(p,wide,h or size*0.55)
        s.ops.append(lambda T,P=p: 'place(center + horizon, dx: %s, dy: %s, text(size: %gpt, $%s$))'%(*T(P,True),size,m))
    def tick(s,p,u,n=3.2,w=0.45):
        "a short stroke across p, square to the direction u: where a measure ends"
        s.line((p[0]-u[1]*n,p[1]+u[0]*n),(p[0]+u[1]*n,p[1]-u[0]*n),w)
    def dim(s,a,b,m,off=(0,0),ticks=False,gap=0,out=False,w=None,h=None):
        """a measure, its label beside the middle; ticks: the book's short strokes at both ends; gap: the line breaks
        that far either side of the middle and the label sits in the break; out: arrows from outside point in at the ends"""
        dx,dy=b[0]-a[0],b[1]-a[1]; L=math.hypot(dx,dy); u=(dx/L,dy/L)
        mid=((a[0]+b[0])/2,(a[1]+b[1])/2)
        if out:
            s.arrow((a[0]-u[0]*8,a[1]-u[1]*8),a,w=0.45,head=2.4); s.arrow((b[0]+u[0]*8,b[1]+u[1]*8),b,w=0.45,head=2.4)
            s.line(a,b,w=0.3,dash='dotted')
        elif gap:
            s.arrow((mid[0]-u[0]*gap,mid[1]-u[1]*gap),a,w=0.45,head=2.4); s.arrow((mid[0]+u[0]*gap,mid[1]+u[1]*gap),b,w=0.45,head=2.4)
        else: s.arrow(a,b,w=0.45,head=2.4,both=True)
        if ticks or out: s.tick(a,u); s.tick(b,u)
        s.label((mid[0]+off[0],mid[1]+off[1]),m,6.5,w,h)
    def dot(s,p,r=1.4):
        s.grow(p,r,r)
        s.ops.append(lambda T,P=p: 'place(center + horizon, dx: %s, dy: %s, circle(radius: %gpt, fill: black, stroke: none))'%(*T(P,True),r))
    def cm(s,p,r=3.2,name='B'):
        "the textbook's centre of mass: a circle, its top-right and bottom-left quarters filled, named B_cm under it"
        for a0 in (0,180): s.path([p]+ell(p[0],p[1],r,r,a0,a0+90,12),closed=True,fill='black',stroke=False)
        s.path(ell(p[0],p[1],r,r),closed=True,w=0.6)
        if name: s.label((p[0]+0.5,p[1]-max(r+5,s.curl+3.6)),'upright(%s)_"cm"'%name,6.5)
    def body(s,tip,name='B',d=(-14,12),ow=1.0):
        """the body's name in a circle, a short hooked arrow from it to a point on its outline; ow is that outline's
        stroke in points: the head stops clear of it, half the stroke plus a hair short of tip"""
        at=(tip[0]+d[0],tip[1]+d[1]); L=math.hypot(*d); u=(d[0]/L,d[1]/L); R=4.6
        g=(ow/2+0.9)/s.SCALE; tip=(tip[0]+u[0]*g,tip[1]+u[1]*g)
        S=(at[0]-u[0]*R,at[1]-u[1]*R); bend=0.22*L*(1 if d[0]*d[1]<0 else -1)
        C=((S[0]+tip[0])/2-u[1]*bend,(S[1]+tip[1])/2+u[0]*bend)
        s.curved(bez(S,C,tip,10),w=0.5,head=2.6)
        s.grow(at,R*s.SCALE,R*s.SCALE)
        s.ops.append(lambda T,P=at: 'place(center + horizon, dx: %s, dy: %s, circle(radius: %gpt, stroke: 0.6pt + black, inset: 0pt, align(center + horizon, text(size: 7pt, $upright(%s)$))))'%(*T(P,True),R*s.SCALE,name))
    def axes(s,p,L=13,z=True,which='xyz',gap=3.2,Ly=None,xl=None,yl=None,zl=None):
        """bold unit vectors from the centre of mass, as the textbook draws them: x right, y up, z toward the reader.
        z="curve" is the book's b̂z out of a flat page: a curled arrow round the mass centre, its head at the left.
        Ly: a shorter b̂y; xl, yl, zl: where a label sits from its arrow's tip (b̂z's from the mass centre)"""
        R=6.4 if z=='curve' else 0
        def v(e,lab,off):
            Le=math.hypot(*e); u=(e[0]/Le,e[1]/Le); g=max(gap,R+1.6)
            if R: e=(e[0]+u[0]*(R-1),e[1]+u[1]*(R-1))
            s.arrow((p[0]+u[0]*g,p[1]+u[1]*g),(p[0]+e[0],p[1]+e[1]),w=1.1,head=4.4)
            s.label((p[0]+e[0]+off[0],p[1]+e[1]+off[1]),bvec(lab),7)
        if 'x' in which: v((L,0),'x',xl or ((-2.5,5.6) if R else (6,0)))
        if 'y' in which: v((0,Ly or L),'y',yl or ((-6.2,-2.5) if R else (-6,-1)))
        if z=='curve':
            s.curl=R
            # from the lower right, over the top, round to the left, where its head points down past the symbol
            A=172; s.path(ell(p[0],p[1],R,R,-58,A+3,32),w=1.3)
            # the book's big head, its base on the arc's end, pointing on along it (down, a little out to the left)
            t=ell(p[0],p[1],R,R,A,A,1)[0]; a=math.radians(A-10); u=(-math.sin(a),math.cos(a)); hl=6.0/s.SCALE
            s.head((t[0]+u[0]*hl,t[1]+u[1]*hl),u,6.0)
            zl=zl or (-R-7.2,1.2); s.label((p[0]+zl[0],p[1]+zl[1]),bvec('z'),7)
        elif z and 'z' in which: v(proj(0,0,L*2.0),'z',(-5,4))
    def typst(s):
        pad=2; x0=min(x for x,_ in s.box)-pad; x1=max(x for x,_ in s.box)+pad
        y0=min(y for _,y in s.box)-pad; y1=max(y for _,y in s.box)+pad
        w,h=x1-x0,y1-y0
        def T(p,pair=False):
            x,y=s.at(p); x,y=x-x0,y1-y
            # a pair: from the box's centre, for what's placed by its own centre
            return ('%.1fpt'%(x-w/2),'%.1fpt'%(y-h/2)) if pair else '(%.1fpt, %.1fpt)'%(x,y)
        return '#box(width: %.1fpt, height: %.1fpt, baseline: %.1fpt, {\n%s\n})'%(w,h,h/2-3.3,'\n'.join(op(T) for op in s.ops))

def W(text):
    "a line of words: drawn in the app's font, copied as it is"
    return {'show':text,'copy':text,'words':True}
# ---- the fluids pictures: the velocity profiles a fluids course draws, arrows from a line of fluid
def hatch(p,x0,x1,y,down=True,step=6,n=4):
    "a wall: its line, short strokes slanting the same way on the solid side"
    p.line((x0,y),(x1,y),w=0.9)
    s=-1 if down else 1
    x=x0+4
    while x<x1-1:
        p.line((x,y),(x+s*n*0.8,y+s*n),w=0.45); x+=step
def profile(p,x,ys,u):
    "arrows from x, each as long as u(y), the line they start on, and the curve through their tips"
    tips=[]
    for y in ys:
        L=u(y); tips.append((x+L,y))
        if L>3: p.arrow((x,y),(x+L,y),w=0.5,head=2.2)
    p.line((x,ys[0]),(x,ys[-1]),w=0.45)
    p.path(tips,w=0.6)
    return tips
# every picture's arrows to one scale: 14 is the average velocity v
V0=14
def pic_uniform():
    p=Pic(); p.SCALE=1.25; profile(p,0,[-12,-6,0,6,12],lambda y:V0)
    p.label((V0+5,0),'v',7.5); return p
def pic_pipe():
    p=Pic(); p.SCALE=1.25; R=14; x1=104
    hatch(p,-4,x1,R,down=False); hatch(p,-4,x1,-R,down=True)
    ys=[-R]+[-R+2+3*i for i in range(9)]+[R]
    # the entrance: flat at v, at rest only at the wall
    profile(p,2,ys,lambda y:V0*(1-(abs(y)/R)**30))
    # the boundary layers grow from each wall and meet: from there on the profile is a parabola, its centre 2v
    for s in (1,-1):
        p.path([(2+70*t,s*R*(1-t**0.5)) for t in [i/20 for i in range(21)] if 2+70*t>V0+4],w=0.4,dash='dashed')
    profile(p,72,ys,lambda y:2*V0*(1-(y/R)**2))
    p.dim((2,-R-9),(72,-R-9),'L_(e)',off=(0,-5.5))
    p.label((22,R+9),'"no slip"',7); p.label((86,R+9),'"fully developed"',7)
    return p
def pic_bl():
    p=Pic(); p.SCALE=1.25; U=V0; L=92
    hatch(p,0,L,0,down=True)
    profile(p,-24,[0,6,12,18,24],lambda y:U)
    p.label((-17,31),'U_(infinity)',7)
    d=lambda x:2.6*x**0.5
    p.path([(x,d(x)) for x in [0,0.5,1,2,3]+list(range(4,L+1,4))],w=0.4,dash='dashed')
    x=56; D=d(x)
    profile(p,x,[D*k/5 for k in range(0,6)],lambda y:U*(2*y/D-(y/D)**2))
    p.line((x,D),(x,D+5),w=0.45); p.arrow((x,D+5),(x+U,D+5),w=0.5,head=2.2)
    xd=L-4; p.arrow((xd,0.8),(xd,d(xd)-0.6),w=0.45,head=2.2,both=True); p.label((xd+6,d(xd)/2),'delta',7)
    p.label((x+8,-11),'"boundary layer"',7)
    return p
PICW=182
def Pf(pic,show,note=None):
    "a picture in a cell of one width, then the row it draws: the row is what a click copies"
    d=I(show,note=note); t=pic.typst(); d['pic']='#box(width: %dpt)[%s]'%(PICW,t); d['picture']=t; return d

continuum=SH('continuum mechanics','Continuum Mechanics','continuum mechanics: stress, beams, torsion, fluids',
 S('Stress and strain',
  G('Simple loading',
    I('σ = F/A',note='normal stress'),
    I('τ = V/A',note='average shear stress'),
    I('ε = ΔL/L',note='normal strain'),
    I('γ = Δx/h',note='shear strain, small angle')),
  G("Plane stress: principal values and Mohr's circle",
    I('σ_{1,2} = (σ_{x} + σ_{y})/2 ± √(((σ_{x} − σ_{y})/2)^{2} + τ_{xy}^{2})',note='principal stresses'),
    I('τ_{max} = √(((σ_{x} − σ_{y})/2)^{2} + τ_{xy}^{2})',note='in plane'),
    I('tan(2θ_{p}) = 2τ_{xy}/(σ_{x} − σ_{y})',note='principal angle'),
    I('C = (σ_{x} + σ_{y})/2',note="Mohr's circle centre"),
    I('R = √(((σ_{x} − σ_{y})/2)^{2} + τ_{xy}^{2})',note="Mohr's circle radius, equal to τ_max in plane")),
  G('Plane stress: rotating by θ',
    I('σ_{x′} = (σ_{x} + σ_{y})/2 + ((σ_{x} − σ_{y})/2) cos(2θ) + τ_{xy} sin(2θ)'),
    I('τ_{x′y′} = −((σ_{x} − σ_{y})/2) sin(2θ) + τ_{xy} cos(2θ)')),
  G('Yield',
    I('σ_{v} = √(((σ_{1} − σ_{2})^{2} + (σ_{2} − σ_{3})^{2} + (σ_{3} − σ_{1})^{2})/2)',note='von Mises'),
    I('σ_{v} = √(σ^{2} + 3τ^{2})',note='von Mises, σ and τ alone'),
    I('τ_{max} = (σ_{1} − σ_{3})/2',note='Tresca, maximum shear'),
    I('n = σ_{Y}/σ_{v}',note='factor of safety against yield'),
    I('K_{t} = σ_{max}/σ_{nom}',note='stress concentration factor')),
  G('As tensors',
    I('t_{i} = σ_{ij} n_{j}',note='traction, Cauchy'),
    I('σ_{ij} = σ_{ji}',note='symmetric'),
    I('∂σ_{ij}/∂x_{j} + ρ b_{i} = ρ a_{i}',note="Cauchy's equation of motion; a = 0 when static"),
    I('ε_{ij} = (1/2)(∂u_{i}/∂x_{j} + ∂u_{j}/∂x_{i})',note='small strain'),
    I('p = −σ_{kk}/3',note='mean pressure'),
    I('s_{ij} = σ_{ij} − (σ_{kk}/3) δ_{ij}',note='deviatoric stress'),
    I('ΔV/V = ε_{kk}',note='dilatation')),
  wide=True),
 S('Constitutive relations',
  G('Isotropic linear elastic',
    I('σ = E ε',note='Hooke, 1D'),
    I('τ = G γ',note='shear'),
    I('ν = −ε_{lat}/ε_{axial}',note='Poisson ratio'),
    I('ε_{x} = (σ_{x} − ν(σ_{y} + σ_{z}))/E + αΔT',note='3D Hooke, with heat'),
    I('γ_{xy} = τ_{xy}/G'),
    I('σ_{ij} = λ ε_{kk} δ_{ij} + 2μ ε_{ij}',note='Lamé form')),
  G('Moduli',
    I('G = E/(2(1 + ν))',note='shear modulus, μ = G'),
    I('K = E/(3(1 − 2ν))',note='bulk modulus'),
    I('λ = Eν/((1 + ν)(1 − 2ν))',note='Lamé constant')),
  G('Newtonian fluid',
    I('τ = μ du/dy',note='μ is viscosity'),
    I('σ_{ij} = −p δ_{ij} + 2μ e_{ij}',note='incompressible'))),
 S('Axial loading and indeterminate problems',
  G('Axial bars',
    I('δ = PL/(AE)',note='elongation'),
    I('k = AE/L',note='stiffness'),
    I('δ_{T} = αΔT L',note='free thermal growth'),
    I('σ = −EαΔT',note='thermal stress, heated and fully restrained'),
    I('U = P^{2}L/(2AE)',note='strain energy in a bar')),
  G('Statically indeterminate: use all three',
    I('Σ F = 0',note='equilibrium'),
    I('Σ δ = 0',note='compatibility, for a closed fit'),
    I('δ = PL/(AE)',note='load and deformation'),
    I('P_{1} = P k_{1}/(k_{1} + k_{2})',note='two bars side by side')),
  G('Pressure vessels (thin wall)',
    I('σ_{h} = pr/t',note='cylinder, hoop'),
    I('σ_{a} = pr/(2t)',note='cylinder, axial'),
    I('σ = pr/(2t)',note='sphere'))),
 S('Torsion',
  G('Circular shafts',
    I('τ = Tρ/J',note='ρ from the centre'),
    I('τ_{max} = Tc/J',note='c is the outer radius'),
    I('φ = TL/(GJ)',note='angle of twist, radians'),
    I('k_{t} = GJ/L',note='torsional stiffness'),
    I('P = Tω',note='power, ω in rad/s')),
  G('Sections',
    I('J = πd^{4}/32',note='solid'),
    I('J = π(d_{o}^{4} − d_{i}^{4})/32',note='hollow'),
    I('τ = T/(2t A_{m})',note='thin wall'),
    I('φ = (TL/(4A_{m}^{2} G)) ∮ ds/t',note='thin wall'))),
 S('Beams',
  G('Bending and shear',
    I('σ = −My/I',note='y up from the neutral axis'),
    I('σ_{max} = Mc/I = M/S',note='S = I/c'),
    I('σ = P/A ± Mc/I',note='combined loading: axial plus bending stress'),
    I('τ = VQ/(It)',note='shear formula, Q = first moment above y'),
    I('q = VQ/I',note='shear flow, force per length'),
    I('τ_{max} = 3V/(2A)',note='rectangle'),
    I('τ_{max} = 4V/(3A)',note='solid circle')),
  G('Load, shear, moment, deflection',
    I('dV/dx = −w',note='w acts downward'),
    I('dM/dx = V'),
    I('EI d^{2}v/dx^{2} = M',note='v up'),
    I('EI d^{4}v/dx^{4} = −w'),
    I('P_{cr} = π^{2}EI/(KL)^{2}',note='Euler buckling: K = 1 pinned, 2 cantilever, 0.5 fixed, 0.7 fixed-pinned'),
    I('σ_{cr} = π^{2}E/(KL/r)^{2}',note='critical buckling stress, slenderness KL/r'),
    I('r = √(I/A)',note='radius of gyration of a section')),
  G('Sections',
    I('I = bh^{3}/12',note='rectangle'),
    I('I = πd^{4}/64',note='solid circle'),
    I('I = π(d_{o}^{4} − d_{i}^{4})/64',note='hollow circle'),
    I('ȳ = (Σ A_{i} y_{i})/(Σ A_{i})',note='neutral axis: centroid of a composite section'),
    I('I = Ī + Ad^{2}',note='parallel axis')),
  G('Largest moment and deflection',
    H('Beam','Moment','Deflection'),
    L('Cantilever, end load P','PL','PL^{3}/(3EI)'),
    L('Cantilever, uniform w','wL^{2}/2','wL^{4}/(8EI)'),
    L('Cantilever, end moment M₀','M_{0}','M_{0}L^{2}/(2EI)'),
    L('Simply supported, centre load P','PL/4','PL^{3}/(48EI)'),
    L('Simply supported, uniform w','wL^{2}/8','5wL^{4}/(384EI)')),
  G('End slope',
    H('Beam','Slope'),
    L('Cantilever, end load P','PL^{2}/(2EI)'),
    L('Cantilever, uniform w','wL^{3}/(6EI)'),
    L('Simply supported, centre load P','PL^{2}/(16EI)'),
    L('Simply supported, uniform w','wL^{3}/(24EI)')),
  wide=True,leads=[280,120]),
 S('Tensor analysis',
  G('Index notation',
    I('a · b = a_{i} b_{i}',note='repeated index sums'),
    I('δ_{ij} a_{j} = a_{i}',note='Kronecker delta'),
    I('(a × b)_{i} = ε_{ijk} a_{j} b_{k}',note='permutation symbol'),
    I('ε_{ijk} ε_{imn} = δ_{jm} δ_{kn} − δ_{jn} δ_{km}'),
    I('σ′_{ij} = Q_{ik} Q_{jl} σ_{kl}',note='rotating a 2nd-order tensor'),
    I('∫_{S} T_{ij} n_{j} dS = ∫_{V} ∂T_{ij}/∂x_{j} dV',note='divergence theorem')),
  G('Principal values',
    I('det(σ − λI) = 0',note='principal stresses'),
    I('I_{1} = σ_{kk}',note='first invariant'),
    I('I_{2} = (σ_{ii} σ_{jj} − σ_{ij} σ_{ij})/2',note='second invariant'),
    I('I_{3} = det(σ)',note='third invariant')),
  wide=True),
 S('Fluid properties and pressure',
  G('Properties',
    I('ρ = m/V',note='density, kg/m³: mass per volume; water 1000 at 4 °C'),
    I('γ = W/V = ρg',note='specific weight, N/m³: weight per volume; water 9.81 kN/m³'),
    I('SG = ρ/ρ_{w} = γ/γ_{w}',note='specific gravity, against water at 4 °C'),
    I('τ = μ du/dy',note='Newtonian fluid: μ dynamic viscosity, Pa·s'),
    I('ν = μ/ρ',note='kinematic viscosity, m²/s'),
    I('p = ρRT',note='ideal gas, absolute p and T: R = 287 J/(kg·K) for air'),
    I('ρ = constant',note='incompressible: liquids, and gases below Mach 0.3'),
    I('Ma = v/c',note='Mach number, c the speed of sound')),
  G('At rest',
    I('dp/dz = −ρg',note='z up'),
    I('p = p_{0} + ρgh',note='hydrostatic pressure, h below the surface'),
    I('p_{abs} = p_{gage} + p_{atm}',note='atmospheric 101.3 kPa at sea level'),
    I('F_{b} = ρgV',note='buoyancy, Archimedes'),
    I('F = p_{c} A',note='on a flat plate: the gage pressure at its centroid times its area'),
    I('y_{cp} = y_{c} + I_{xc}/(y_{c} A)',note='centre of pressure, y down the slant from the free surface'))),
 S('Flow rate and velocity profiles',
  G('Flow rate',
    I('Q = A v',note='volume flow rate, m³/s: A perpendicular to the flow, v the uniform or average velocity'),
    I('ṁ = ρQ = ρ A v',note='mass flow rate, kg/s'),
    I('ṁ = ∫_{A} ρ u dA',note='non-uniform: u the local velocity normal to A; as vectors ∫ ρ (v · n) dA'),
    I('v = (1/A) ∫_{A} u dA',note='average velocity')),
  G('Velocity profiles',
    Pf(pic_uniform(),'u = v',note='uniform: the same speed across the flow'),
    Pf(pic_pipe(),'u = 2v (1 − r^{2}/R^{2})',note='internal flow: no slip, u = 0 at the wall; laminar and fully developed past the entrance length'),
    I('L_{e}/D ≈ 0.06 Re',note='entrance length, laminar'),
    I('L_{e}/D ≈ 4.4 Re^{1/6}',note='entrance length, turbulent'),
    Pf(pic_bl(),'δ ≈ 5x/√(Re_{x})',note='external flow: laminar boundary layer on a flat plate; viscosity matters inside δ'),
    I('Re_{x} = ρU_{∞}x/μ',note='turbulent past about 500,000')),
  wide=True),
 S('Bernoulli, pipe flow and drag',
  G('Bernoulli: steady, incompressible, no friction',
    I('p + ρv^{2}/2 + ρgz = constant',note='along a streamline'),
    I('A_{1} v_{1} = A_{2} v_{2}',note='continuity, incompressible'),
    I('v = √(2gh)',note="Torricelli: outflow from a tank, h below the free surface"),
    I('v = √(2(p_{0} − p)/ρ)',note='Pitot tube: p₀ stagnation, p static pressure')),
  G('Pipe flow',
    I('Re = ρvD/μ',note='Reynolds number: laminar below about 2300'),
    I('Q = πR^{4}Δp/(8μL)',note='Poiseuille: laminar, horizontal, Δp the drop over L'),
    I('f = 64/Re',note='laminar friction factor'),
    I('1/√f = −1.8 log_{10}((ε/(3.7D))^{1.11} + 6.9/Re)',note='Haaland: turbulent, ε/D the relative roughness'),
    I('D_{h} = 4A/P_{w}',note='hydraulic diameter: 4 × area over wetted perimeter')),
  G('Head loss and energy',
    I('h_{f} = f (L/D) v^{2}/(2g)',note='Darcy-Weisbach head loss'),
    I('h_{m} = K v^{2}/(2g)',note='minor losses: fittings, valves, bends'),
    I('p_{1}/γ + v_{1}^{2}/(2g) + z_{1} + h_{p} = p_{2}/γ + v_{2}^{2}/(2g) + z_{2} + h_{t} + h_{L}',note='energy in heads: the pump adds head, a turbine takes it, losses add up'),
    I('P = ρ g Q h',note='pump (hydraulic) power')),
  G('Drag and lift',
    I('F_{D} = (1/2) C_{D} ρ v^{2} A',note='drag force: A frontal area, v speed past the body'),
    I('F_{L} = (1/2) C_{L} ρ v^{2} A',note='lift force: A planform (wing) area'))),
 S('Conservation laws on a control volume',
  G('In general',
    I('dB/dt = d/dt ∫_{CV} ρb dV + ∫_{CS} ρb (v · n) dA',note='Reynolds transport'),
    I('0 = d/dt ∫_{CV} ρ dV + ∫_{CS} ρ (v · n) dA',note='mass'),
    I('Σ F = d/dt ∫_{CV} ρv dV + ∫_{CS} ρv (v · n) dA',note='momentum'),
    I('Σ M = d/dt ∫_{CV} ρ(r × v) dV + ∫_{CS} ρ(r × v)(v · n) dA',note='angular momentum, M the moments'),
    I('Q̇ − Ẇ = d/dt ∫_{CV} ρe dV + ∫_{CS} ρ(e + p/ρ)(v · n) dA',note='energy, e = û + v²/2 + gz, û internal energy')),
  G('Steady, one way in and one out',
    I('ṁ = ρ_{1} A_{1} v_{1} = ρ_{2} A_{2} v_{2}',note='mass'),
    I('Σ F = ṁ (v_{2} − v_{1})',note='momentum, as vectors'),
    I('Q̇ − Ẇ = ṁ (h_{2} − h_{1} + (v_{2}^{2} − v_{1}^{2})/2 + g(z_{2} − z_{1}))',note='energy, h enthalpy')),
  G('At a point',
    I('∂ρ/∂t + ∇ · (ρv) = 0',note='continuity'),
    I('ρ Dv/Dt = −∇p + μ∇^{2}v + ρg',note='Navier-Stokes, incompressible')),
  wide=True))

precalc=SH('precalculus','Precalculus','precalculus: lines, quadratics, factoring, exponents and logs, interest, functions, trigonometry, conics, sequences',
 S('Lines and points',
  G('Lines',
    I('m = (y_{2} − y_{1})/(x_{2} − x_{1})',note='slope, rise over run'),
    I('y = m x + b',note='slope-intercept form'),
    I('y − y_{1} = m(x − x_{1})',note='point-slope form'),
    I('m_{1} m_{2} = −1',note='perpendicular; parallel: m₁ = m₂')),
  G('Points',
    I('d = √((x_{2} − x_{1})^{2} + (y_{2} − y_{1})^{2})',note='distance formula'),
    I('((x_{1} + x_{2})/2, (y_{1} + y_{2})/2)',note='midpoint formula'))),
 S('Quadratics and polynomials',
  G('Quadratics',
    I('x = (−b ± √(b^{2} − 4ac))/(2a)',note='quadratic formula, roots of ax² + bx + c = 0'),
    I('Δ = b^{2} − 4ac',note='discriminant: > 0 two real roots, 0 one, < 0 complex'),
    I('x = −b/(2a)',note='vertex, axis of symmetry'),
    I('y = a(x − h)^{2} + k',note='vertex form, vertex (h, k)'),
    I('a x^{2} + b x + c = a(x + b/(2a))^{2} + c − b^{2}/(4a)',note='completing the square'),
    I('r_{1} + r_{2} = −b/a,  r_{1} r_{2} = c/a',note="Vieta: sum and product of the roots")),
  G('Factoring',
    I('a^{2} − b^{2} = (a + b)(a − b)',note='difference of squares'),
    I('a^{3} + b^{3} = (a + b)(a^{2} − ab + b^{2})',note='sum of cubes'),
    I('a^{3} − b^{3} = (a − b)(a^{2} + ab + b^{2})',note='difference of cubes'),
    I('(a + b)^{n} = Σ_{k=0}^{n} C(n, k) a^{n−k}b^{k}',note='binomial theorem'),
    I('p(x) = (x − r) q(x) + p(r)',note='remainder and factor theorem: p(r) = 0 means x − r is a factor')),
  wide=True),
 S('Exponents and logs',
  G('Exponents',
    I('a^{m}a^{n} = a^{m+n}'),
    I('a^{m}/a^{n} = a^{m−n}'),
    I('(a^{m})^{n} = a^{mn}'),
    I('a^{−n} = 1/a^{n}'),
    I('a^{1/n} = ^{n}√a',note='nth root')),
  G('Logs',
    I('log_{b}(x) = y ⇔ b^{y} = x',note='definition: a log is an exponent'),
    I('log_{b}(xy) = log_{b}(x) + log_{b}(y)',note='product rule'),
    I('log_{b}(x/y) = log_{b}(x) − log_{b}(y)',note='quotient rule'),
    I('log_{b}(x^{n}) = n log_{b}(x)',note='power rule'),
    I('log_{b}(x) = ln(x)/ln(b)',note='change of base')),
  G('Growth and decay',
    I('y = y_{0} e^{kt}',note='exponential growth (k > 0) or decay (k < 0)'),
    I('A = P(1 + r/n)^{n t}',note='interest compounded n times a year'),
    I('A = P e^{r t}',note='continuous compounding'),
    I('t_{1/2} = ln(2)/k',note='half-life of a decay y₀e⁻ᵏᵗ, k > 0'),
    I('t_{d} = ln(2)/r',note='doubling time of growth y₀eʳᵗ'))),
 S('Functions',
  G('Graphs and inverses',
    I('y = a f(b(x − h)) + k',note='transformations: shift h right and k up, stretch by a, squeeze x by b'),
    I('f(−x) = f(x)',note='even: symmetric about the y-axis'),
    I('f(−x) = −f(x)',note='odd: symmetric about the origin'),
    I('(f ∘ g)(x) = f(g(x))',note='composition'),
    I('f(f^{−1}(x)) = x',note='inverse: swap x and y, solve for y'),
    I('(f(x + h) − f(x))/h',note='difference quotient: average rate of change')),
  G('Rational functions and absolute value',
    I('f(x) = p(x)/q(x)',note='asymptotes: vertical where q = 0; horizontal from the leading terms'),
    I('|x − a| < b ⇔ a − b < x < a + b',note='absolute value inequality; > b splits in two')),
  G('Polar coordinates',
    I('x = r cos(θ)'),
    I('y = r sin(θ)'),
    I('r^{2} = x^{2} + y^{2}',note='polar from rectangular'),
    I('θ = atan2(y, x)',note='angle in the right quadrant')),
  wide=True),
 S('Trigonometry',
  G('Right triangles and the unit circle',
    I('sin θ = opp/hyp,  cos θ = adj/hyp,  tan θ = opp/adj',note='SOH CAH TOA',copy='sin(θ) = opp/hyp'),
    I('θ_{rad} = θ_{deg} π/180',note='degrees to radians'),
    I('sin^{2}x + cos^{2}x = 1',note='Pythagorean identity',copy='sin(x)^2 + cos(x)^2 = 1'),
    I('tan x = sin x / cos x',copy='tan(x) = sin(x)/cos(x)'),
    I('y = A sin(B(x − C)) + D',note='sinusoid graph: amplitude |A|, period 2π/B, phase shift C, midline D'),
    I('s = rθ',note='arc length, θ in radians')),
  G('Any triangle',
    I('a/sin(A) = b/sin(B) = c/sin(C)',note='law of sines'),
    I('c^{2} = a^{2} + b^{2} − 2ab cos(C)',note='law of cosines'),
    I('A = (1/2) a b sin(C)',note='triangle area, C between a and b')),
  wide=True),
 S('Conic sections',
  G('Centred at (h, k) or the origin',
    I('(x − h)^{2} + (y − k)^{2} = r^{2}',note='circle, centre (h, k), radius r'),
    I('x^{2}/a^{2} + y^{2}/b^{2} = 1',note='ellipse, semi-axes a and b'),
    I('c^{2} = a^{2} − b^{2}',note='ellipse foci at (±c, 0), a > b'),
    I('x^{2}/a^{2} − y^{2}/b^{2} = 1',note='hyperbola, opens left and right'),
    I('c^{2} = a^{2} + b^{2}',note='hyperbola foci at (±c, 0)'),
    I('y = ±(b/a) x',note='hyperbola asymptotes'),
    I('y^{2} = 4 p x',note='parabola opening right: focus (p, 0), directrix x = −p'),
    I('x^{2} = 4 p y',note='parabola opening up: focus (0, p), directrix y = −p'),
    I('e = c/a',note='eccentricity: 0 circle, < 1 ellipse, 1 parabola, > 1 hyperbola')),
  wide=True),
 S('Sequences and series',
  G('Arithmetic (common difference d)',
    I('a_{n} = a_{1} + (n − 1) d',note='arithmetic sequence, nth term'),
    I('S_{n} = n(a_{1} + a_{n})/2',note='arithmetic series sum')),
  G('Geometric (common ratio r)',
    I('a_{n} = a_{1} r^{n−1}',note='geometric sequence, nth term'),
    I('S_{n} = a_{1}(1 − r^{n})/(1 − r)',note='geometric series sum, r ≠ 1'),
    I('S = a_{1}/(1 − r)',note='infinite geometric series, |r| < 1'))))

diffeq=SH('differential equations','Differential Equations','differential equations: first order, second order, vibrations, forcing, Laplace, systems, PDEs',
 S('First order',
  G('Separable, and the models it solves',
    I('∫ dy/g(y) = ∫ f(t) dt',note='separable: dy/dt = f(t) g(y)'),
    I('y = y_{0} e^{kt}',note='dy/dt = k y: exponential growth or decay'),
    I('dN/dt = r N (1 − N/K)',note='logistic growth, carrying capacity K'),
    I('dT/dt = −k (T − T_{a})',note="Newton's law of cooling"),
    I('dQ/dt = c_{in} r_{in} − (Q/V) r_{out}',note='mixing tank: salt in minus salt out, V the volume')),
  G('Linear',
    I('dy/dt + p y = q',note='linear first order; p and q may depend on t'),
    I('μ = e^{∫ p dt}',note='integrating factor'),
    I('y = (1/μ)(∫ μ q dt + C)',note='general solution')),
  G('Bernoulli and exact',
    I('dy/dt + p y = q y^{n}',note='Bernoulli equation: v = y^{1−n} makes it linear'),
    I('M dx + N dy = 0',note='exact if ∂M/∂y = ∂N/∂x; then F_x = M, F_y = N, F = C')),
  wide=True),
 S('Second order',
  G('Constant coefficients',
    I('a y′′ + b y′ + c y = 0',note='homogeneous, try y = e^{rt}'),
    I('a r^{2} + b r + c = 0',note='characteristic (auxiliary) equation'),
    I('y′′ + 2ζω_{n} y′ + ω_{n}^{2} y = 0',note='damped oscillator, standard form')),
  G('Solution by roots',
    H('Roots','Solution'),
    L('Real and distinct','y = c_{1} e^{r_{1}t} + c_{2} e^{r_{2}t}'),
    L('Repeated','y = (c_{1} + c_{2} t) e^{rt}'),
    L('Complex, α ± jβ','y = e^{αt} (c_{1} cos(βt) + c_{2} sin(βt))')),
  G('Spring-mass-damper (mechanical vibrations)',
    I('m x′′ + c x′ + k x = 0',note='free vibration, mass-spring-damper'),
    I('ω_{n} = √(k/m)',note='natural frequency'),
    I('c_{c} = 2√(k m)',note='critical damping'),
    I('ζ = c/c_{c}',note='damping ratio: < 1 underdamped, 1 critical, > 1 overdamped'),
    I('ω_{d} = ω_{n} √(1 − ζ^{2})',note='damped natural frequency')),
  G('Variable coefficients',
    I('a t^{2} y′′ + b t y′ + c y = 0',note='Euler-Cauchy (equidimensional): try y = t^{r}'),
    I('a r(r − 1) + b r + c = 0',note='its indicial equation'),
    I('y_{2} = y_{1} ∫ e^{−∫ p dt}/y_{1}^{2} dt',note="reduction of order: a second solution from y₁, for y′′ + p y′ + q y = 0")),
  wide=True,leads=[180]),
 S('Forced (nonhomogeneous)',
  G('The whole solution',
    I('y = y_{h} + y_{p}',note='homogeneous plus one particular solution'),
    I('W = y_{1} y′_{2} − y_{2} y′_{1}',note='Wronskian: W ≠ 0 means independent solutions'),
    I('y_{p} = −y_{1} ∫ y_{2} g/W dt + y_{2} ∫ y_{1} g/W dt',note='variation of parameters, for y′′ + p y′ + q y = g')),
  G('Undetermined coefficients: guess y_p, times t if it solves the homogeneous equation',
    H('Forcing g(t)','Guess y_p'),
    C('t^{n}','A_{n} t^{n} + … + A_{1} t + A_{0}'),
    C('e^{at}','A e^{at}'),
    C('cos(ωt), sin(ωt)','A cos(ωt) + B sin(ωt)'),
    C('e^{at} cos(ωt)','e^{at} (A cos(ωt) + B sin(ωt))'),
    C('cos(ω_{0}t), sin(ω_{0}t)','t (A cos(ω_{0}t) + B sin(ω_{0}t))',note='resonance: forced at the natural frequency, times t')),
  wide=True,leads=[180]),
 S('Laplace transform method',
  G('Transform the equation, solve for Y(s), invert by partial fractions',
    I('ℒ{y′} = s Y − y(0)',note='first derivative, initial value'),
    I('ℒ{y′′} = s^{2} Y − s y(0) − y′(0)',note='second derivative, initial values'),
    I('ℒ{u(t − a) f(t − a)} = e^{−as} F(s)',note='step (Heaviside) forcing switched on at t = a'),
    I('ℒ{δ(t − a)} = e^{−as}',note='impulse (Dirac delta) at t = a')),
  wide=True),
 S('Systems',
  G('Linear, constant coefficients',
    I('x′ = A x',note='linear system'),
    I('x = v e^{λt}',note='eigen-solution: A v = λ v'),
    I('x = c_{1} v_{1} e^{λ_{1}t} + c_{2} v_{2} e^{λ_{2}t}',note='general solution, distinct eigenvalues')),
  G('Eigenvalues and the phase portrait (T = trace, D = det)',
    I('det(A − λI) = 0',note='characteristic equation of the system'),
    I('λ = T/2 ± √(T^{2}/4 − D)',note='2 × 2: both < 0 stable node, opposite signs saddle, complex spiral, imaginary centre')),
  wide=True),
 S('Partial differential equations',
  G('The classics',
    I('∂u/∂t = α ∂^{2}u/∂x^{2}',note='heat (diffusion) equation'),
    I('∂^{2}u/∂t^{2} = c^{2} ∂^{2}u/∂x^{2}',note='wave equation'),
    I('∂^{2}u/∂x^{2} + ∂^{2}u/∂y^{2} = 0',note="Laplace's equation, steady state")),
  G('Solving',
    I('u = X(x) T(t)',note='separation of variables'),
    I('u = f(x − ct) + g(x + ct)',note="d'Alembert: the wave equation's general solution")),
  wide=True))

numerical=SH('numerical methods','Numerical Methods','numerical methods: roots, interpolation, differences, quadrature, ODE solvers, linear systems, error',
 S('Roots',
  G('Iterations',
    I('x_{n+1} = x_{n} − f(x_{n})/f′(x_{n})',note='Newton-Raphson, converges quadratically'),
    I('x_{n+1} = x_{n} − f(x_{n})(x_{n} − x_{n−1})/(f(x_{n}) − f(x_{n−1}))',note='secant method, no derivative'),
    I('c = (a + b)/2',note='bisection: keep the half where f changes sign'),
    I('|e_{n}| ≤ (b − a)/2^{n}',note='bisection error after n halvings'),
    I('n ≥ log_{2}((b − a)/ε)',note='bisection: iterations needed for a tolerance ε'),
    I('x_{n+1} = g(x_{n})',note='fixed-point iteration, converges if |g′| < 1'),
    I('x_{k+1} = x_{k} − J^{−1}(x_{k}) F(x_{k})',note='Newton for a system: J the Jacobian, solve J Δx = −F')),
  wide=True),
 S('Interpolation and fitting',
  G('Through points',
    I('y = y_{0} + (y_{1} − y_{0})(x − x_{0})/(x_{1} − x_{0})',note='linear interpolation'),
    I('p(x) = Σ_{i=0}^{n} y_{i} Π_{j≠i} (x − x_{j})/(x_{i} − x_{j})',note='Lagrange interpolating polynomial'),
    I('f[x_{0}, x_{1}] = (f(x_{1}) − f(x_{0}))/(x_{1} − x_{0})',note='divided difference, first order'),
    I('p(x) = f[x_{0}] + f[x_{0}, x_{1}](x − x_{0}) + f[x_{0}, x_{1}, x_{2}](x − x_{0})(x − x_{1}) + …',note="Newton's divided-difference interpolating polynomial"),
    I('x = (A^{T} A)^{−1} A^{T} b',note='least squares, normal equations')),
  wide=True),
 S('Derivatives and integrals',
  G('Finite differences',
    I('f′(x) ≈ (f(x + h) − f(x))/h',note='forward difference, error O(h)'),
    I('f′(x) ≈ (f(x) − f(x − h))/h',note='backward difference, error O(h)'),
    I('f′(x) ≈ (f(x + h) − f(x − h))/(2h)',note='central difference, error O(h²)'),
    I('f′′(x) ≈ (f(x + h) − 2f(x) + f(x − h))/h^{2}',note='second derivative, central, error O(h²)')),
  G('Integrals (quadrature), h = (b − a)/n',
    I('∫ f dx ≈ h Σ_{i=0}^{n−1} f(x_{i} + h/2)',note='midpoint rule, error O(h²)'),
    I('∫ f dx ≈ (h/2)(f_{0} + 2f_{1} + 2f_{2} + … + f_{n})',note='trapezoid rule, error O(h²)'),
    I('∫ f dx ≈ (h/3)(f_{0} + 4f_{1} + 2f_{2} + 4f_{3} + … + f_{n})',note="Simpson's 1/3 rule, n even, error O(h⁴)"),
    I('∫ f dx ≈ (3h/8)(f_{0} + 3f_{1} + 3f_{2} + f_{3})',note="Simpson's 3/8 rule, one panel of 3 intervals"),
    I('∫_{−1}^{1} f dx ≈ f(−1/√3) + f(1/√3)',note='Gauss-Legendre quadrature, 2 points, exact for cubics'),
    I('E_{T} = −(b − a) h^{2} f′′(ξ)/12',note='trapezoid rule error, ξ in [a, b]'),
    I('E_{S} = −(b − a) h^{4} f^{(4)}(ξ)/180',note="Simpson's rule error, ξ in [a, b]")),
  wide=True),
 S('ODE solvers',
  G('One step at a time, y′ = f(t, y)',
    I('y_{n+1} = y_{n} + h f(t_{n}, y_{n})',note='Euler (explicit, forward), error O(h)'),
    I('y_{n+1} = y_{n} + h f(t_{n+1}, y_{n+1})',note='backward Euler (implicit), error O(h), stable for stiff equations'),
    I('y_{n+1} = y_{n} + (h/2)(f(t_{n}, y_{n}) + f(t_{n+1}, y_{n} + h f(t_{n}, y_{n})))',note="Heun's method (improved Euler, RK2), O(h²)")),
  G('Runge-Kutta 4 (RK4)',
    I('k_{1} = h f(t_{n}, y_{n})',note='stage 1, slope at the start'),
    I('k_{2} = h f(t_{n} + h/2, y_{n} + k_{1}/2)',note='stage 2, slope at the middle'),
    I('k_{3} = h f(t_{n} + h/2, y_{n} + k_{2}/2)',note='stage 3, middle again'),
    I('k_{4} = h f(t_{n} + h, y_{n} + k_{3})',note='stage 4, slope at the end'),
    I('y_{n+1} = y_{n} + (k_{1} + 2k_{2} + 2k_{3} + k_{4})/6',note='RK4 step, error O(h⁴)')),
  wide=True),
 S('Linear systems and error',
  G('Matrices',
    I('A = L U',note='LU decomposition: solve L y = b, then U x = y'),
    I('x_{i}^{(k+1)} = (b_{i} − Σ_{j≠i} a_{ij} x_{j}^{(k)})/a_{ii}',note='Jacobi iteration'),
    I('x_{i}^{(k+1)} = (b_{i} − Σ_{j<i} a_{ij} x_{j}^{(k+1)} − Σ_{j>i} a_{ij} x_{j}^{(k)})/a_{ii}',note='Gauss-Seidel: uses the new values at once'),
    I('x_{k+1} = A x_{k}/‖A x_{k}‖',note='power method: the largest eigenvalue'),
    I('κ(A) = ‖A‖ ‖A^{−1}‖',note='condition number: lose about log₁₀ κ digits')),
  G('Error',
    I('|x − x_{a}|',note='absolute error'),
    I('|x − x_{a}|/|x|',note='relative error'),
    I('|e_{n+1}| ≈ C |e_{n}|^{p}',note='order p (1 linear, 2 quadratic)')),
  wide=True))

def up(m):
    "the book sets its measures upright: r, a, b, L, θ"
    return re.sub(r'([A-Za-zθπ])',r'upright(\1)',m) if len(m)==1 else m
def clear_of(p,ow,gap=0.9):
    "how far a head or tip stops short of an outline ow points thick: half the stroke plus a clear gap, in picture units"
    return (ow/2+gap)/Pic.SCALE
def radius_dim(p,O,ang,r,ow,lab,off):
    """the book's radius: a thin line from the centre out to the outline at ang degrees, a head at each end (the outer
    one stopping clear of the outline's stroke), and a short tick at the centre, square to it, on the far side"""
    u=(math.cos(math.radians(ang)),math.sin(math.radians(ang))); k=r-clear_of(p,ow)
    p.arrow(O,(O[0]+u[0]*k,O[1]+u[1]*k),w=0.45,head=2.4,both=True)
    n=(u[1],-u[0]); p.line((O[0]-n[0]*0.8,O[1]-n[1]*0.8),(O[0]+n[0]*5.5,O[1]+n[1]*5.5),w=0.45)
    p.label((O[0]+u[0]*r*0.62+off[0],O[1]+u[1]*r*0.62+off[1]),up(lab),6.5)
def by_through(p,yc,top,ow):
    "b̂y's length so it runs up from the mass centre (its curl) through an outline at height top and its head clears it"
    return top+clear_of(p,ow)+4.4/Pic.SCALE+0.4-yc-5.4
def pic_rod():
    p=Pic(); L=48
    p.line((-L,0),(L,0),w=1.8)
    p.axes((0,0),L=13,z="curve",yl=(5.8,-1.6),zl=(-10,5.5)); p.cm((0,0)); p.body((L-6,0),d=(8,9),ow=1.8)
    p.dim((-L,-19),(L,-19),up('L'),ticks=True,gap=5); return p
def pic_arc(fill=False,sector=False):
    r,th=(52,55) if sector else (46,50); p=Pic(); O=(0,0); ow=0.9 if sector else 2.0
    a0,a1=90-th,90+th; arc=ell(O[0],O[1],r,r,a0,a1); e0,e1=arc[0],arc[-1]
    # the sector's centre is where it is; the arc's is drawn lower than true, as the book draws it, so its b̂z curl
    # and labels sit clear under the thick arc
    yc=(2*r*math.sin(math.radians(th))/(3*math.radians(th))) if sector else r-12.5
    if sector: p.path([O]+arc,closed=True,fill=GREY,w=ow)
    else:
        p.path(arc,w=ow)
        for e in (e0,e1): p.line(O,e,w=0.6)
    c=(0,yc)
    # the book's guides: a dotted line from the vertex up to the mass centre, a dashed one level with it, out past the arc
    p.line(O,(0,yc),w=0.45,dash='dotted')
    hx=r*math.sin(math.radians(th)); xr=math.sqrt(r*r-yc*yc)
    p.line((-hx-12,yc),(xr+(2 if sector else 7),yc),w=0.45,dash='dashed')
    ra,at=(13,8.6) if sector else (17,11)
    p.angle(O,ra,90,a1,'upright(θ)',at=at); p.angle(O,ra,90,a0,'upright(θ)',at=at)
    # r along the right-hand side, set off from it, its label in a break in the line; the centre's height on the left
    n=(math.cos(math.radians(a0-90)),math.sin(math.radians(a0-90))); k=7
    p.dim((O[0]+n[0]*k,O[1]+n[1]*k),(e0[0]+n[0]*k,e0[1]+n[1]*k),up('r'),ticks=True,gap=4.5)
    m='(2 upright(r) sin(upright(θ)))/(3 upright(θ))' if sector else '(upright(r) sin(upright(θ)))/upright(θ)'
    p.dim((-hx-8,yc),(-hx-8,0),m,ticks=True,gap=7.5,w=14 if sector else 11,h=9)
    if sector: p.axes(c,L=11,z="curve",yl=(5.6,-1),zl=(-13.6,4.2))
    else: p.axes(c,L=11,z="curve",Ly=by_through(p,yc,r,ow),xl=(-1.5,5.2),yl=(5.8,1.6),zl=(-13.6,4.2))
    p.cm(c,name=''); p.label((c[0]-9.5,c[1]-9.2),'upright(B)_"cm"',6.5)  # beside the dotted drop, not on it
    p.body(ell(O[0],O[1],r,r,a0+12,a0+12,1)[0],d=(10,8),ow=ow)
    return p
def pic_semi(fill=False):
    r=44 if fill else 38; p=Pic(); O=(0,0); ow=0.9 if fill else 2.0
    arc=ell(O[0],O[1],r,r,0,180)
    yc=4*r/(3*math.pi) if fill else 2*r/math.pi
    if fill: p.path(arc,closed=True,fill=GREY,w=ow); p.line(O,(0,r),w=0.45,dash='dotted')
    else: p.path(arc,w=ow); p.line((-r,0),(r,0),w=0.45,dash='dashed'); p.line(O,(0,yc),w=0.45,dash='dotted')
    c=(0,yc)
    if fill:
        p.line((-r-12,yc),(r,yc),w=0.45,dash='dashed')
        p.dim((-r-7,yc),(-r-7,0),'(4 upright(r))/(3 upright(π))',(-9,0),out=True,w=8,h=9)
    else:
        hx=-r*0.52; p.line((-math.sqrt(r*r-yc*yc),yc),(math.sqrt(r*r-yc*yc),yc),w=0.45,dash='dashed')
        p.dim((hx,yc),(hx,0),'(2 upright(r))/upright(π)',ticks=True,gap=6.5,w=5,h=9)
    radius_dim(p,O,35 if fill else 42,r,ow,'r',(3.5,-4.5))
    if fill: p.axes(c,L=11,z="curve",yl=(5.6,-1),zl=(-13.6,4.2))
    else: p.axes(c,L=11,z="curve",Ly=by_through(p,yc,r,ow),yl=(5.8,1.6),zl=(-13.6,4.2))
    p.cm(c,name=''); p.label((c[0]-9.5,c[1]-9.2),'upright(B)_"cm"',6.5)  # beside the dotted drop, not on it
    p.body(ell(O[0],O[1],r,r,62,62,1)[0],d=(10,8),ow=ow)
    return p
def pic_circle(fill=False,a=30,b=30):
    p=Pic(); pts=ell(0,0,a,b,n=72)
    if fill:
        # the disk's edge, a sliver of thickness showing on the lower right
        # the shifted copy's part outside the face, then back along the face between where they cross
        back=ell(1.5,-0.9,a,b,-210,150,360)[:-1]; out=[(x/a)**2+(y/b)**2>1 for x,y in back]
        k=out.index(False); back=back[k:]+back[:k]; out=out[k:]+out[:k]
        i0=out.index(True); i1=len(out)-out[::-1].index(True); edge=back[i0:i1]
        t0,t1=[math.degrees(math.atan2(q[1]/b,q[0]/a)) for q in (edge[0],edge[-1])]
        if t0>t1: t0-=360
        sliver=edge+ell(0,0,a,b,t1,t0,36)
        p.path(sliver,closed=True,fill=GREY,stroke=False); p.path(sliver,closed=True,fill=GREY,stroke=False)
        p.path(edge,w=0.8)
        p.path(pts,closed=True,fill=GREY,w=0.8)
    else: p.path(pts,closed=True,w=2.4)
    p.axes((0,0),L=12,z="curve"); p.cm((0,0))
    p.body(ell(0,0,a,b,128,128,1)[0],d=(-9,8),ow=0.8 if fill else 2.4)
    if a==b:
        # to the disk's rim, a tick across it; the hoop's thick ring is its own stop: the head ends just clear of it
        u=(math.cos(math.radians(52)),math.sin(math.radians(52))); k=a if fill else a-2.4/2/Pic.SCALE-0.7; e=(u[0]*k,u[1]*k)
        g0,g1=a*0.50,a*0.66
        p.line((-u[0]*4.8,-u[1]*4.8),(u[0]*g0,u[1]*g0),w=0.45); p.arrow((u[0]*g1,u[1]*g1),e,w=0.45,head=2.8)
        if fill: p.tick(e,u,n=3.0,w=0.6)
        p.label((u[0]*(g0+g1)/2,u[1]*(g0+g1)/2),up('r'),6.5)
    else:
        x=-a-8; p.dim((x,b),(x,0),up('b'),ticks=True,gap=4.5); p.dim((x,0),(x,-b),up('b'),ticks=True,gap=4.5)
        y=-b-8; p.dim((-a,y),(0,y),up('a'),ticks=True,gap=4.5); p.dim((0,y),(a,y),up('a'),ticks=True,gap=4.5)
    return p
def pic_plate(a=56,b=44):
    p=Pic(); ox,oy=1.4,-1.4
    # its edge: a thin strip along the right and the bottom
    lip=[(-a/2,-b/2),(a/2,-b/2),(a/2,b/2),(a/2+ox,b/2+oy),(a/2+ox,-b/2+oy),(-a/2+ox,-b/2+oy)]
    p.path(lip,closed=True,fill=GREY,stroke=False); p.path(lip,closed=True,fill=GREY,w=0.6)
    p.path([(-a/2,-b/2),(a/2,-b/2),(a/2,b/2),(-a/2,b/2)],closed=True,fill=GREY,w=0.8)
    p.axes((0,0),L=12,z="curve"); p.cm((0,0)); p.body((a/2+ox,b/2-7),d=(9,6))
    p.dim((-a/2-7,b/2),(-a/2-7,-b/2),up('b'),ticks=True,gap=4.5)
    p.dim((-a/2,-b/2-8),(a/2,-b/2-8),up('a'),ticks=True,gap=4.5); return p
def upr(m):
    "a measure set upright, as the book sets r, h, a, b, c and their parts"
    return re.sub(r'([A-Za-z])',r'upright(\1)',m)
def out_of_page(p,c):
    "the end of an axis that points at the reader: a small ring with a dot, as the book marks a cylinder's face"
    p.path(ell(c[0],c[1],1.9,1.9,n=20),closed=True,w=0.5); p.dot(c,0.75)
def gap_dim(p,a,b,m,gap=3.5,w=0.45,clear=1.4):
    """the book's measure along a radius: a line from the centre, the name in a gap, an arrow out toward the edge,
    its head stopping clear of the outline, and the book's tick across the line at the edge"""
    d=(b[0]-a[0],b[1]-a[1]); L=math.hypot(*d); u=(d[0]/L,d[1]/L); mid=(a[0]+d[0]/2,a[1]+d[1]/2)
    p.line(a,(mid[0]-u[0]*gap,mid[1]-u[1]*gap),w=w)
    p.arrow((mid[0]+u[0]*gap,mid[1]+u[1]*gap),(b[0]-u[0]*clear,b[1]-u[1]*clear),w=w,head=2.4); p.tick(b,u)
    p.label(mid,m,6.5)
def radius_line(p,c,e,w=0.45,clear=1.4):
    "a radius as the book draws it: from the centre itself, through the symbol there, its head clear of the rim, a tick at the rim"
    d=(e[0]-c[0],e[1]-c[1]); L=math.hypot(*d); u=(d[0]/L,d[1]/L)
    p.arrow(c,(e[0]-u[0]*clear,e[1]-u[1]*clear),w=w,head=2.4); p.tick(e,u)
def cyl_outline(p,r,D,inner=None,shell=False):
    """the book's cylinder: its near face toward the reader, a little narrower than tall, the length going back to
    the right (D, the far face's centre). the side is shaded, darker below its axis; returns the face's half-width
    and the bottom tangent's angle"""
    rx=r*0.88; L=math.hypot(*D); n=(-D[1]/L,D[0]/L)
    tt=math.degrees(math.atan2(r*n[1],rx*n[0])); tb=tt-180
    side=ell(D[0],D[1],rx,r,tt,tb,36)+ell(0,0,rx,r,tb,tt-360,36)
    p.path(side,closed=True,fill=GREY,w=0.9,holes=[ell(0,0,rx,r)])
    p.path(ell(0,0,rx,r,0,tb,16)+ell(D[0],D[1],rx,r,tb,0,16),closed=True,fill=GREY,stroke=False)
    face=ell(0,0,rx,r)
    if inner: p.path(face,closed=True,fill=GREY,w=0.9,holes=[ell(0,0,inner*0.88,inner)])
    else: p.path(face,closed=True,fill=None if shell else GREY,w=0.9)
    return rx,tb
def pic_cyl(kind='solid'):
    r=26; D=(46,4); p=Pic(); ri=22; thick=kind=='thick'
    rx,tb=cyl_outline(p,r,D,inner=ri if thick else None,shell=kind=='thin')
    # as the book draws it: the axes on the near face's centre, b_z curling out of the page
    p.axes((0,0),L=12,z='curve',xl=(-4.8,-5.8)); out_of_page(p,(0,0))
    def rim(t,R=r): return (R*0.88*math.cos(math.radians(t)),R*math.sin(math.radians(t)))
    def radius(e,lab,at): radius_line(p,(0,0),e); p.label(at,upr(lab),6.5)
    if thick: radius(rim(78,ri),'r_i',(7,15)); radius(rim(42),'r_o',(14.5,8.5))
    else: radius(rim(58),'r',(12.6,14.4))
    p.body(rim(125,r+1.6),d=(-8,9))
    # h along the bottom of the side, off it a little, square to the length; the name in a break in the line
    L=math.hypot(*D); n=(D[1]/L,-D[0]/L); q=rim(tb); off=7
    a=(q[0]+n[0]*off,q[1]+n[1]*off); b=(a[0]+D[0],a[1]+D[1])
    p.dim(a,b,upr('h'),ticks=True,gap=4.5)
    return p
def pic_cone(thin=False):
    r=34; h=62; ry=9; p=Pic(); apex=(0,h); f=3 if thin else 4; c=(0,h/f)
    if thin:
        p.path(ell(0,0,r,ry,0,180),w=0.5,dash='dashed')
        p.path(ell(0,0,r,ry,180,360)+[apex],closed=True,w=0.9)
    else:
        p.path([apex]+ell(0,0,r,ry,180,360),closed=True,fill=GREY,w=0.9)
        p.path([apex]+ell(0,0,r,ry,290,360,12),closed=True,fill=GREY,stroke=False)
        p.path(ell(0,0,r,ry,0,180),w=0.5,dash='dashed')
    p.line((0,0),(0,h-1),w=0.4,dash='dotted'); p.dot((0,0),1.1)
    p.axes(c,L=11,z=False,xl=(-1.5,5.4)); p.cm(c,name='')
    gap_dim(p,(0,0),(r,0),upr('r'))
    n=(h/math.hypot(h,r),r/math.hypot(h,r)); p.body((r*0.38+n[0]*1.6,h*0.62+n[1]*1.6),d=(11,7))
    # the book's heights: the centre of mass's level carried out to the left, the two parts measured there
    X=-r-7
    p.line((X-2,h),(-2,h),w=0.4,dash='dashed'); p.line((X-2,c[1]),(c[0]-4,c[1]),w=0.4,dash='dashed'); p.line((X-2,0),(-r,0),w=0.4,dash='dashed')
    up_,dn=('2h\\/3','h\\/3') if thin else ('3h\\/4','h\\/4')
    p.dim((X,c[1]),(X,h),upr(up_),(-8,0)); p.dim((X,0),(X,c[1]),upr(dn),(-6.5,0))
    return p
def ccw_curl(p,c,R,a0,a1,w=1.3,head=6.0):
    "the book's b̂z round a sphere's mass centre: a curl counter-clockwise from a0 to a1 degrees, its head on at a1"
    p.path(ell(c[0],c[1],R,R,a0,a1,32),w=w)
    t=ell(c[0],c[1],R,R,a1,a1,1)[0]; a=math.radians(a1+12); u=(-math.sin(a),math.cos(a)); hl=head/p.SCALE
    p.head((t[0]+u[0]*hl,t[1]+u[1]*hl),u,head)
def arrows_both(p,a,b,m,gap=3.5,clear_a=1.4,clear_b=1.4,w=0.45,off=(0,0)):
    """the book's measure with its name in a break: an arrow out from the name each way, the heads stopping
    clear_a and clear_b short of the ends (clear of a symbol or a thick outline)"""
    d=(b[0]-a[0],b[1]-a[1]); L=math.hypot(*d); u=(d[0]/L,d[1]/L); mid=(a[0]+d[0]/2+off[0]*u[0],a[1]+d[1]/2+off[0]*u[1])
    p.arrow((mid[0]-u[0]*gap,mid[1]-u[1]*gap),(a[0]+u[0]*clear_a,a[1]+u[1]*clear_a),w=w,head=2.4)
    p.arrow((mid[0]+u[0]*gap,mid[1]+u[1]*gap),(b[0]-u[0]*clear_b,b[1]-u[1]*clear_b),w=w,head=2.4)
    p.label(mid,m,6.5)
def pic_sphere(fill=True):
    r=30; c=12; p=Pic(); O=(0,0)
    p.path(ell(0,0,r,r),closed=True,fill=GREY if fill else None,w=0.9)
    p.path(ell(0,0,r,c,180,360),w=0.7); p.path(ell(0,0,r,c,0,180),w=0.6,dash='dashed')
    # as the book draws a sphere: b_y and b_x from the mass centre, b_z curling under it, its head at the upper right
    p.axes(O,L=22,Ly=20,z=False,which='xy',gap=9,xl=(-2.2,4.6),yl=(5.2,-2.4))
    ccw_curl(p,O,6.4,205,382); p.label((9.6,6.4),bvec('z'),7)
    p.cm(O,name=''); p.label((-13.5,1.6),'upright(B)_"cm"',6.5)
    # r from the mass centre up to the rim on the upper left, a head at each end, its name in a break clear of the equator
    e=ell(0,0,r,r,129,129,1)[0]; arrows_both(p,O,e,upr('r'),gap=3.2,clear_a=4.2,clear_b=1.2,off=(4.2,0))
    p.body(ell(0,0,r,r,55,55,1)[0],d=(10,9))
    return p
def pic_ellipsoid(fill=True):
    a,b,c=34,24,12; p=Pic()
    if fill: p.path(ell(0,0,a,b),closed=True,fill=GREY,w=0.9)
    else: p.path(ell(0,0,a,b),closed=True,w=0.9); p.path(ell(0,0,a-1.8,b-1.8),closed=True,w=0.6)
    p.path(ell(0,0,a,c,180,360),w=0.7); p.path(ell(0,0,a,c,0,180),w=0.6,dash='dashed')
    p.axes((0,0),L=15,Ly=13,z='curve',yl=(5.6,-1.5)); p.cm((0,0),name=''); p.label((12,-4.4),'upright(B)_"cm"',6.5)
    p.body(ell(0,0,a,b,55,55,1)[0],d=(10,9))
    # the book's half-measures: b and b up the left, a and a along the bottom, c and c on the right
    X=-a-8; Y=-b-8; X2=a+6
    p.dim((X,b),(X,0),upr('b'),ticks=True,gap=4.5); p.dim((X,0),(X,-b),upr('b'),ticks=True,gap=4.5)
    p.dim((-a,Y),(0,Y),upr('a'),ticks=True,gap=4.5); p.dim((0,Y),(a,Y),upr('a'),ticks=True,gap=4.5)
    for y in (c,-c): p.line((a*0.3,y),(X2+2.5,y),w=0.4,dash='dotted')
    p.line((a+1,0),(X2+2.5,0),w=0.4,dash='dotted')
    p.dim((X2,0),(X2,c),upr('c'),(4,0)); p.dim((X2,-c),(X2,0),upr('c'),(4,0))
    return p
def pic_hemi():
    r=34; ry=9; p=Pic(); B=(0,0); c=(0,3*r/8)
    p.path(ell(0,0,r,r,0,180)+ell(0,0,r,ry,180,360),closed=True,fill=GREY,w=1.2)
    p.path(ell(0,0,r,ry,0,180),w=0.6,dash='dashed')
    # as the book draws it: b_y up from the centre of mass, b_x along the flat face from B_o
    p.axes(c,L=12,z=False,which='y'); p.axes(B,L=22,z=False,which='x')
    p.cm(c,name=''); p.label((10.5,c[1]+0.6),'upright(B)_"cm"',6.5); p.dot(B); p.label((0.5,-4.8),'upright(B)_upright(o)',6)
    e=ell(0,0,r,r,25,25,1)[0]; p.arrow((2.2*0.906,2.2*0.423),e,w=0.45,head=2.4); p.label((24,14.6),upr('r'),6.5)
    p.body(ell(0,0,r,r,55,55,1)[0],d=(10,8))
    X=-r-7
    p.line((X-2,c[1]),(c[0]-4,c[1]),w=0.4,dash='dashed'); p.line((X-2,0),(-2,0),w=0.4,dash='dashed')
    p.dim((X,0),(X,c[1]),upr('(3r)/8'),(-7,0))
    return p
def pic_parab():
    r=26; a=64; rx=7; p=Pic(); y0=-0.42*r
    prof=[(a*(1-(t/r)**2),t) for t in [r*i/24 for i in range(-24,25)]]
    p.path(ell(0,0,rx,r,90,270)+prof,closed=True,fill=GREY,w=0.9)
    s=math.degrees(math.asin(-y0/r))
    p.path(ell(0,0,rx,r,180+s,270,12)+[q for q in prof if q[1]<y0]+[(a*(1-(y0/r)**2),y0)],closed=True,fill=GREY,stroke=False)
    p.path(ell(0,0,rx,r,-90,90),w=0.4,dash='dotted')
    c=(a/3,0); p.line((0,0),(a,0),w=0.4,dash='dashed')
    p.axes(c,L=14,z=False); p.cm(c,name=''); p.label((c[0]+8,-6.3),'upright(B)_"cm"',6.5)
    p.dot((0,0)); p.label((-12,0),'upright(B)_upright(o)',6)
    p.arrow((0,2),(0,r),w=0.45,head=2.4); p.label((-3.2,r*0.55),upr('r'),6.5)
    Y=-r-7
    p.line((a/3,-3.4),(a/3,Y-2),w=0.4,dash='dotted'); p.line((a,0),(a,Y-2),w=0.4,dash='dotted'); p.line((0,-r),(0,Y-2),w=0.4)
    p.dim((0,-13),(a/3,-13),upr('a/3'),gap=4); p.dim((0,Y),(a,Y),upr('a'),gap=4.5)
    p.body((a*0.5,r*math.sqrt(0.5)),d=(8,10))
    return p
def pic_hemi():
    r=34; ry=17; p=Pic(); B=(0,0); c=(0,3*r/8*0.83)
    p.path(ell(0,0,r,r,0,180)+ell(0,0,r,ry,180,360),closed=True,fill=GREY,w=1.2)
    p.path(ell(0,0,r,ry,0,180),w=0.6,dash='dashed')
    # as the book draws it: B_o's level dashed across the flat face, b_x along it out to the rim, b_y high in the dome
    p.line((-r-9,0),(-1.8,0),w=0.4,dash='dashed'); p.line((1.8,0),(19,0),w=0.4,dash='dashed')
    p.axes((0,19.4),L=11.4,z=False,which='y',gap=1.6,yl=(-6.2,-3.4)); p.axes(B,L=31.4,z=False,which='x',gap=19.5,xl=(-7.5,4.8))
    p.cm(c,name=''); p.label((-6.6,c[1]-5.8),'upright(B)_"cm"',6.5); p.dot(B); p.label((0.5,-5),'upright(B)_upright(o)',6)
    # r from B_o up to the rim on the right, a head at each end, its name in a break
    e=ell(0,0,r,r,40,40,1)[0]; arrows_both(p,B,e,upr('r'),gap=3,clear_a=2.2,clear_b=1.6,off=(-2.6,0))
    p.body(ell(0,0,r,r,62,62,1)[0],d=(10,8))
    # 3r/8 at the left: the mass centre's level carried out, arrows from outside pointing in, the name between
    X=-r-6
    p.line((X-3,c[1]),(c[0]-4.4,c[1]),w=0.4,dash='dashed')
    p.arrow((X,c[1]+9),(X,c[1]),w=0.45,head=2.4); p.arrow((X,-9),(X,0),w=0.45,head=2.4)
    p.label((X,c[1]/2),upr('(3r)/8'),6.5)
    return p
def pic_parab():
    r=26; a=64; rx=7; p=Pic(); y0=-0.42*r
    prof=[(a*(1-(t/r)**2),t) for t in [r*i/24 for i in range(-24,25)]]
    p.path(ell(0,0,rx,r,90,270)+prof,closed=True,fill=GREY,w=0.9)
    s=math.degrees(math.asin(-y0/r))
    p.path(ell(0,0,rx,r,180+s,270,12)+[q for q in prof if q[1]<y0]+[(a*(1-(y0/r)**2),y0)],closed=True,fill=GREY,stroke=False)
    p.path(ell(0,0,rx,r,-90,90),w=0.4,dash='dotted')
    c=(a/3,0); p.line((1.8,0),(c[0]-4.2,0),w=0.4,dash='dashed'); p.line((c[0]+28,0),(a-1,0),w=0.4,dash='dashed')
    p.axes(c,L=24,z=False,gap=4.2,Ly=15,xl=(-9,4.8),yl=(6,-3))
    p.cm(c,name=''); p.label((c[0]+8,-6.3),'upright(B)_"cm"',6.5)
    p.dot((0,0)); p.label((-13,-0.6),'upright(B)_upright(o)',6)
    # r up the flat face from B_o to its top, a head at each end, its name in a break
    arrows_both(p,(0,0),(0,r),upr('r'),gap=3.2,clear_a=1.9,clear_b=1.6)
    # as the book: dashed drops from B_o, the mass centre and the tip to the measures below
    Y=-r-7; Y3=-0.7*r
    p.line((0,-1.9),(0,Y-2),w=0.4,dash='dashed'); p.line((a/3,-4.2),(a/3,Y3-2),w=0.4,dash='dashed'); p.line((a,-0.6),(a,Y-2),w=0.4,dash='dashed')
    p.dim((0,Y3),(a/3,Y3),upr('a/3'),gap=4); p.dim((0,Y),(a,Y),upr('a'),gap=4.5)
    p.body((a*0.5,r*math.sqrt(0.5)),d=(8,10))
    return p
def hatch(p,poly,ang=55,gap=5.5,w=0.4,skip=()):
    "the book's hatching: parallel lines across a convex outline, broken where a symbol or label sits (skip: x0, y0, x1, y1)"
    u=(math.cos(math.radians(ang)),math.sin(math.radians(ang))); n=(-u[1],u[0])
    ds=[q[0]*n[0]+q[1]*n[1] for q in poly]; d=min(ds)+gap/2
    while d<max(ds):
        ts=[]
        for i in range(len(poly)):
            a,b=poly[i],poly[(i+1)%len(poly)]
            da,db=a[0]*n[0]+a[1]*n[1]-d,b[0]*n[0]+b[1]*n[1]-d
            if da*db<0:
                f=da/(da-db); q=(a[0]+(b[0]-a[0])*f,a[1]+(b[1]-a[1])*f); ts.append(q[0]*u[0]+q[1]*u[1])
        if len(ts)==2:
            t0,t1=sorted(ts); pt=lambda t:(n[0]*d+u[0]*t,n[1]*d+u[1]*t)
            # the parts of the line left after the skipped boxes are cut out
            keep=[(t0,t1)]
            for x0,y0,x1,y1 in skip:
                cut=[]
                for k0,k1 in keep:
                    lo,hi=k0,k1
                    for c,lo_,hi_ in ((0,x0,x1),(1,y0,y1)):
                        if abs(u[c])<1e-9:
                            if not lo_<n[c]*d<hi_: lo,hi=1,0
                            continue
                        e0,e1=sorted(((lo_-n[c]*d)/u[c],(hi_-n[c]*d)/u[c])); lo,hi=max(lo,e0),min(hi,e1)
                    if lo>=hi: cut.append((k0,k1)); continue
                    if lo>k0: cut.append((k0,lo))
                    if hi<k1: cut.append((hi,k1))
                keep=cut
            for k0,k1 in keep:
                if k1-k0>0.8: p.line(pt(k0),pt(k1),w=w)
        d+=gap
def box_faces(V,tones):
    "the box's three seen faces, front, top, right, each its own grey as the book shades them"
    return [([V[q] for q in face],'rgb(128, 128, 128, %d)'%t) for face,t in zip([[(0,0,1),(1,0,1),(1,1,1),(0,1,1)],[(0,1,1),(1,1,1),(1,1,0),(0,1,0)],[(1,0,1),(1,0,0),(1,1,0),(1,1,1)]],tones)]
def pic_box(particles=False):
    # the book's box: seen from the front left and a little above, a (x) running right and slightly down, c (z) toward
    # the reader down and to the left; no outlines, only the three seen faces' greys, the hidden edges dashed and
    # b̂x b̂y b̂z from the hidden back bottom corner, as the book draws them; a b c measured, ticked at their ends
    X,Y,Z=(67.7,-8.6),(0,30.8),(-25.9,-8.8); p=Pic()
    o=(-(X[0]+Y[0]+Z[0])/2,-(X[1]+Y[1]+Z[1])/2)
    V={(i,j,k):(o[0]+i*X[0]+j*Y[0]+k*Z[0],o[1]+i*X[1]+j*Y[1]+k*Z[1]) for i in (0,1) for j in (0,1) for k in (0,1)}
    for face,fill in box_faces(V,(150,110,255)): p.path(face,closed=True,fill=fill,stroke=False)
    H=V[0,0,0]
    for q in [(1,0,0),(0,1,0),(0,0,1)]: p.line(H,V[q],w=0.4,dash='dashed')
    if particles:
        for q in V.values(): p.dot(q,2.2)
    def unit(e): L=math.hypot(*e); return (e[0]/L,e[1]/L)
    def sh(q,o,k=1): return (q[0]+o[0]*k,q[1]+o[1]*k)
    def m(u,v): return ((u[0]+v[0])/2,(u[1]+v[1])/2)
    for e,n,L,off in [(X,'x',12.6,(0.5,5.2)),(Y,'y',13,(5.4,-3.6)),(Z,'z',11.5,(2.2,7.4))]:
        u=unit(e); g=2.6 if particles else 0; tip=sh(H,u,L)
        p.arrow(sh(H,u,g),tip,w=1.1,head=4.4); p.label(sh(tip,off),bvec(n),7)
    p.body(sh(V[0,0,1],Y,0.72),d=(-12,-5),ow=0)
    # a below the front's bottom edge, b up the front's right edge (its name on the side face), c along the side's
    # bottom edge going back; the 8 particles sit at (±a, ±b, ±c), so there each is measured from the middle out, twice
    A0,A1=sh(V[0,0,1],(0,-5.5)),sh(V[1,0,1],(0,-5.5))
    zu=unit(Z); co=(-zu[1]*5,zu[0]*5)
    if co[1]>0: co=(-co[0],-co[1])
    C0,C1=sh(V[1,0,1],co),sh(V[1,0,0],co)
    B0,B1=sh(V[1,0,1],(0.9,0)),sh(V[1,1,1],(0.9,0))
    if not particles:
        p.dim(A0,A1,up('a'),(0,-0.6),ticks=True,gap=5); p.dim(B0,B1,up('b'),(4.6,0),ticks=True); p.dim(C0,C1,up('c'),(0,-0.6),ticks=True,gap=4.2)
    else:
        for u,v,n,g in [(A0,A1,'a',4.5),(C0,C1,'c',3.2)]:
            for s0,s1 in ((u,m(u,v)),(m(u,v),v)):
                if g: p.dim(s0,s1,up(n),(0,-0.6),ticks=True,gap=g)
                else: p.dim(s0,s1,up(n),sh(co,(0,0),0.9),ticks=True)
        # b's heads stop clear of the particles at its ends; its middle ticked
        M=m(B0,B1); r=2.2/p.SCALE+0.7
        for s0,s1 in ((sh(B0,(0,r)),M),(M,sh(B1,(0,-r)))): p.dim(s0,s1,up('b'),(4.6,0))
        p.tick(M,(0,1))
    return p
def pic_triangle():
    # the book's triangle: O at the left, p⃗ down to P, q⃗ up to Q, hatched, its mass centre a third of the way out
    p=Pic(); O=(0,0); P_=(42,-33); Q=(61,31)
    c=((P_[0]+Q[0])/3,(P_[1]+Q[1])/3)
    hatch(p,[O,P_,Q],ang=61,gap=5.3,skip=[(c[0]-4.6,c[1]-4.6,c[0]+4.6,c[1]+4.6)])
    p.line(P_,Q,w=0.8); p.arrow(O,P_,w=1.3,head=5); p.arrow(O,Q,w=1.3,head=5)
    p.cm(c,name='')
    for q in (O,P_,Q): p.dot(q,1.8)
    p.body((Q[0]*0.37,Q[1]*0.37),d=(-9,6),ow=1.3)
    p.label((-1.5,5),'upright(O)',7); p.label((P_[0]+4.6,P_[1]-0.6),'upright(P)',7); p.label((Q[0]+4,Q[1]+3.6),'upright(Q)',7)
    p.label((P_[0]-10,P_[1]+1),'arrow(bold(upright(p)))',7); p.label((Q[0]-10,Q[1]+2),'arrow(bold(upright(q)))',7); return p
def pic_tetra():
    # the book's tetrahedron: O P Q hatched underneath, O Q R shaded above it, p⃗ q⃗ r⃗ out from O, P R a thin edge
    p=Pic(); O=(0,0); P_=(30,-23.3); Q=(41.4,21.7); R=(27.8,54)
    p.path([O,Q,R],closed=True,fill='rgb(128, 128, 128, 150)',stroke=False)
    hatch(p,[O,P_,Q],ang=44,gap=5)
    p.line(P_,Q,w=0.7); p.line(Q,R,w=0.8); p.line(P_,R,w=0.5)
    for e in (P_,Q,R): p.arrow(O,e,w=1.3,head=5)
    for q in (O,P_,Q,R): p.dot(q,1.8)
    p.body((R[0]*0.66,R[1]*0.66),d=(-12,-1),ow=1.3)
    p.label((-2.5,3.8),'upright(O)',7); p.label((P_[0]+4.2,P_[1]-0.4),'upright(P)',7); p.label((Q[0]+4,Q[1]+3.4),'upright(Q)',7); p.label((R[0]+4.4,R[1]-0.8),'upright(R)',7)
    p.label((P_[0]-9,P_[1]+0.8),'arrow(bold(upright(p)))',7); p.label((Q[0]-7,Q[1]+3),'arrow(bold(upright(q)))',7); p.label((R[0]-6.4,R[1]-3.6),'arrow(bold(upright(r)))',7)
    return p

def Pc(pic,show,note=None):
    "a body's picture, then its volume (or the first of its formulas): the volume is what a click copies"
    d=I(show,note=note); d['pic']=pic.typst(); return d
def U(name,value):
    "a constant: its name in words, its value as a value with its unit"
    d=L(name,value); d['unit']=True; return d
def body_group(title,pic,first,*rows,note=None):
    return G(title,Pc(pic,first,note),*rows)

# Mitiguy's notation: ^{N}v⃗^{Q} is Q's velocity in N, I⃡ an inertia dyadic, B∕O (a division slash) "B about O"
dynamics=SH('dynamics','Statics and Dynamics','statics and dynamics: vectors, rotation, kinematics, inertia, momentum, energy, equations of motion, friction, vibrations',
 S('Vectors',
  G('Dot and cross products',
    I('a⃗ · b⃗ = |a⃗| |b⃗| cos(θ)',note='dot product'),
    I('a⃗ · b⃗ = b⃗ · a⃗'),
    I('a⃗ · b⃗ = a_{x}b_{x} + a_{y}b_{y} + a_{z}b_{z}'),
    I('|v⃗| = √(v⃗ · v⃗)',note='magnitude'),
    I('v^{2} = |v⃗|^{2} = v⃗ · v⃗',note='magnitude squared'),
    I('v̂ = v⃗/|v⃗|',note='unit vector'),
    I('θ = acos((a⃗ · b⃗)/(|a⃗| |b⃗|))',note='angle between'),
    I('(a⃗ · û) û',note='part of a⃗ along û'),
    I('a⃗ × b⃗ = |a⃗| |b⃗| sin(θ) û',note='cross product, û ⊥ both'),
    I('a⃗ × b⃗ = −b⃗ × a⃗'),
    I('a⃗ × b⃗ = (a_{y}b_{z} − a_{z}b_{y}) x̂ + (a_{z}b_{x} − a_{x}b_{z}) ŷ + (a_{x}b_{y} − a_{y}b_{x}) ẑ'),
    I('a⃗ × b⃗ = det[x̂ ŷ ẑ; a_{x} a_{y} a_{z}; b_{x} b_{y} b_{z}]'),
    I('x̂ × ŷ = ẑ,  ŷ × ẑ = x̂,  ẑ × x̂ = ŷ',note='right-handed'),
    I('a⃗ × a⃗ = 0⃗,  a⃗ · (a⃗ × b⃗) = 0')),
  G('Triple products and more',
    I('a⃗ × (b⃗ × c⃗) = b⃗ (a⃗ · c⃗) − c⃗ (a⃗ · b⃗)',note='triple cross: BAC − CAB'),
    I('(a⃗ × b⃗) × c⃗ = b⃗ (a⃗ · c⃗) − a⃗ (b⃗ · c⃗)'),
    I('a⃗ · b⃗ × c⃗ = a⃗ × b⃗ · c⃗',note='scalar triple: swap · and ×'),
    I('a⃗ · b⃗ × c⃗ = b⃗ · c⃗ × a⃗ = c⃗ · a⃗ × b⃗',note='cyclic; the box volume'),
    I('|a⃗ × b⃗|^{2} = |a⃗|^{2} |b⃗|^{2} − (a⃗ · b⃗)^{2}',note='Lagrange'),
    I('(a⃗ × b⃗) · (c⃗ × d⃗) = (a⃗ · c⃗)(b⃗ · d⃗) − (a⃗ · d⃗)(b⃗ · c⃗)'),
    I('a⃗ × (b⃗ + c⃗) = a⃗ × b⃗ + a⃗ × c⃗')),
  G('Derivatives',
    I('^{A}dv⃗/dt = v̇_{1} â_{1} + v̇_{2} â_{2} + v̇_{3} â_{3}',note='â₁, â₂, â₃ fixed in A'),
    I('^{A}d(a⃗ · b⃗)/dt = ^{A}da⃗/dt · b⃗ + a⃗ · ^{A}db⃗/dt'),
    I('^{A}d(a⃗ × b⃗)/dt = ^{A}da⃗/dt × b⃗ + a⃗ × ^{A}db⃗/dt',note='keep the order'),
    I('^{A}d(s a⃗)/dt = ṡ a⃗ + s ^{A}da⃗/dt')),
  wide=True),
 S('Rotation matrices',
  G('Rotation table',
    I('^{a}R^{b}_{ij} = â_{i} · b̂_{j}',note='rows â, columns b̂'),
    I('^{b}R^{a} = (^{a}R^{b})^{−1} = (^{a}R^{b})^{T}',note='orthogonal'),
    I('^{a}R^{d} = ^{a}R^{b} ^{b}R^{c} ^{c}R^{d}',note='chain them')),
  G('B turned θ about one axis of A',
    I('R_{z} = [cos θ, −sin θ, 0; sin θ, cos θ, 0; 0, 0, 1]',note='â_{z} = b̂_{z}'),
    I('R_{x} = [1, 0, 0; 0, cos θ, −sin θ; 0, sin θ, cos θ]',note='â_{x} = b̂_{x}'),
    I('R_{y} = [cos θ, 0, sin θ; 0, 1, 0; −sin θ, 0, cos θ]',note='â_{y} = b̂_{y}'),
    I('b̂_{x} = cos θ â_{x} + sin θ â_{y}',note='about z (SohCahToa)'),
    I('b̂_{y} = −sin θ â_{x} + cos θ â_{y}')),
  wide=True),
 S('Angular velocity and acceleration',
  G('Between frames A, B, C, D',
    I('^{A}dv⃗/dt = ^{B}dv⃗/dt + ^{A}ω⃗^{B} × v⃗',note='golden rule'),
    I('^{A}ω⃗^{B} = ±θ̇ λ̂',note='λ̂ fixed in both A and B'),
    I('^{A}ω⃗^{D} = ^{A}ω⃗^{B} + ^{B}ω⃗^{C} + ^{C}ω⃗^{D}',note='addition theorem'),
    I('^{A}ω⃗^{B} = −^{B}ω⃗^{A}'),
    I('^{A}α⃗^{B} = ^{A}d(^{A}ω⃗^{B})/dt = ^{B}d(^{A}ω⃗^{B})/dt',note='either frame')),
  wide=True),
 S('Position, velocity and acceleration',
  G('Position and distance',
    I('^{P}r⃗^{Q} = −^{Q}r⃗^{P}',note='negation'),
    I('^{Q_{1}}r⃗^{Q_{4}} = ^{Q_{3}}r⃗^{Q_{4}} + ^{Q_{2}}r⃗^{Q_{3}} + ^{Q_{1}}r⃗^{Q_{2}}',note='addition'),
    I('d = |^{P}r⃗^{Q}| = √(^{P}r⃗^{Q} · ^{P}r⃗^{Q})',note='distance P to Q')),
  G('Velocity and acceleration in N',
    I('^{N}v⃗^{Q} = ^{N}d(^{N_{o}}r⃗^{Q})/dt'),
    I('^{N}a⃗^{Q} = ^{N}d(^{N}v⃗^{Q})/dt'),
    I('^{N}v⃗^{Q} = ^{N}v⃗^{B_{o}} + ^{N}ω⃗^{B} × ^{B_{o}}r⃗^{Q}',note='Bₒ and Q fixed on B'),
    I('^{N}a⃗^{Q} = ^{N}a⃗^{B_{o}} + ^{N}α⃗^{B} × ^{B_{o}}r⃗^{Q} + ^{N}ω⃗^{B} × (^{N}ω⃗^{B} × ^{B_{o}}r⃗^{Q})',note='Bₒ and Q fixed on B'),
    I('^{N}v⃗^{Q} = ^{N}v⃗^{B_{o}} + ^{N}ω⃗^{B} × ^{B_{o}}r⃗^{Q} + ^{B}v⃗^{Q}',note='Q moving on B'),
    I('^{N}a⃗^{Q} = ^{N}a⃗^{B_{o}} + ^{N}α⃗^{B} × r⃗ + ^{N}ω⃗^{B} × (^{N}ω⃗^{B} × r⃗) + 2 ^{N}ω⃗^{B} × ^{B}v⃗^{Q} + ^{B}a⃗^{Q}',note='Q moving on B; 2ω × v is Coriolis')),
  G('Polar and path coordinates',
    I('v⃗ = ṙ r̂ + r θ̇ θ̂',note='polar'),
    I('a⃗ = (r̈ − r θ̇^{2}) r̂ + (r θ̈ + 2 ṙ θ̇) θ̂',note='polar'),
    I('a⃗ = v̇ t̂ + (v^{2}/ρ) n̂',note='along a path, radius ρ')),
  wide=True),
 S('Constraints',
  G('Rolling and gears',
    I('^{F}v⃗^{B_{A}} = ^{F}v⃗^{A_{B}}',note='rolling: the touching points'),
    I('^{F}ω^{A} r_{A} = ± ^{F}ω^{B} r_{B}',note='gears A and B'),
    I('v = ω r',note='disk rolling, no slip'),
    I('a = α r',note='its centre, no slip')),
  G('Mass and centre of mass',
    I('m^{S} = Σ_{i=1}^{n} m^{Q_{i}}'),
    I('^{O}r⃗^{S_{cm}} = (1/m^{S}) Σ_{i=1}^{n} m^{Q_{i}} ^{O}r⃗^{Q_{i}}'),
    I('m^{S} ^{N}v⃗^{S_{cm}} = Σ_{i=1}^{n} m^{Q_{i}} ^{N}v⃗^{Q_{i}}'),
    I('m^{S} ^{N}a⃗^{S_{cm}} = Σ_{i=1}^{n} m^{Q_{i}} ^{N}a⃗^{Q_{i}}'))),
 S('Inertia dyadics',
  G('Moments and products',
    I('I⃡^{Q∕O} = m^{Q} [1⃡ (^{O}r⃗^{Q} · ^{O}r⃗^{Q}) − ^{O}r⃗^{Q} ^{O}r⃗^{Q}]',note='particle Q about O'),
    I('I⃡^{S∕O} = Σ_{i=1}^{n} I⃡^{Q_{i}∕O}',note='a system: add them'),
    I('I_{ûû}^{S∕O} = û · I⃡^{S∕O} · û',note='moment, û a unit vector'),
    I('I_{ûv̂}^{S∕O} = û · I⃡^{S∕O} · v̂',note='product, û ⊥ v̂'),
    I('I_{ûû}^{S∕O} = Σ_{i=1}^{n} m^{Q_{i}} d_{i}^{2}',note='d_{i} = |^{O}r⃗^{Q_{i}} × û|'),
    I('I_{ûv̂}^{S∕O} = −Σ_{i=1}^{n} m^{Q_{i}} δ_{u_{i}} δ_{v_{i}}',note='δ_{u_{i}} = ^{O}r⃗^{Q_{i}} · û')),
  G('Shift (parallel axis) theorems',
    I('I⃡^{S∕P} = I⃡^{S∕S_{cm}} + m^{S} [1⃡ (^{P}r⃗^{S_{cm}} · ^{P}r⃗^{S_{cm}}) − ^{P}r⃗^{S_{cm}} ^{P}r⃗^{S_{cm}}]',note='dyadic'),
    I('I_{ûû}^{S∕O} = I_{ûû}^{S∕S_{cm}} + m^{S} d^{2}',note='d = |^{O}r⃗^{S_{cm}} × û|'),
    I('I_{xy}^{S∕O} = I_{xy}^{S∕S_{cm}} − m^{S} x_{cm} y_{cm}',note='product; x, y of the mass centre from O')),
  G('As a matrix',
    I('[I] = [I_{xx}, I_{xy}, I_{xz}; I_{xy}, I_{yy}, I_{yz}; I_{xz}, I_{yz}, I_{zz}]',note='on the axes b̂x, b̂y, b̂z'),
    I('I_{xx} = Σ m (y^{2} + z^{2})'),
    I('I_{xy} = −Σ m x y',note="Mitiguy's sign"),
    I('I_{ûû} = [u]^{T} [I] [u]',note='any direction û'),
    I('det([I] − λ [1]) = 0',note='principal moments λ'),
    I('I_{zz} = I_{xx} + I_{yy}',note='flat body in x-y (perpendicular axis)'),
    I('k = √(I/m)',note='radius of gyration')),
  wide=True),
 S('Moments of inertia',
  G('Every body below',
    I('I_{xy} = I_{xz} = I_{yz} = 0',note='on the axes drawn (not the triangle or tetrahedron)'),
    I('I = Ī + m d^{2}',note='parallel axis')),
  G('About the centre, unless it says',
    H('Body','I'),
    L('Solid sphere','(2/5) m r^{2}'),
    L('Hollow sphere','(2/3) m r^{2}'),
    L('Solid cylinder','(1/2) m r^{2}'),
    L('Hoop','m r^{2}'),
    L('Rod','(1/12) m L^{2}'),
    L('Rod, about one end','(1/3) m L^{2}')),
  wide=True,leads=[180]),
 S('Inertia of rods and wires',
  body_group('Line or thin rod',pic_rod(),'I_{xx} = 0',I('I_{yy} = I_{zz} = (1/12) m L^{2}')),
  body_group('Circular arc',pic_arc(),'L = 2 r θ',
    I('y_{cm} = r sin(θ)/θ',note='from the centre'),
    I('I_{xx} = (1/2) m r^{2} [1 + sin(2θ)/(2θ) − 2 sin^{2}(θ)/θ^{2}]'),
    I('I_{yy} = (1/2) m r^{2} [1 − sin(2θ)/(2θ)]'),
    I('I_{zz} = m r^{2} [1 − sin^{2}(θ)/θ^{2}]',note='= I_{xx} + I_{yy}')),
  body_group('Semicircle',pic_semi(),'L = π r',
    I('y_{cm} = 2r/π',note='from the centre'),
    I('I_{xx} = (1/2) m r^{2} (1 − 8/π^{2})'),
    I('I_{yy} = (1/2) m r^{2}'),
    I('I_{zz} = m r^{2} (1 − 4/π^{2})',note='= I_{xx} + I_{yy}')),
  body_group('Circle or thin hoop',pic_circle(),'L = 2π r',
    I('I_{xx} = I_{yy} = (1/2) m r^{2}'),
    I('I_{zz} = m r^{2}'))),
 S('Inertia of flat plates',
  body_group('Circular sector',pic_arc(sector=True),'A = θ r^{2}',
    I('y_{cm} = 2r sin(θ)/(3θ)',note='from the apex'),
    I('I_{xx} = (1/4) m r^{2} [1 + sin(2θ)/(2θ) − 16 sin^{2}(θ)/(9θ^{2})]'),
    I('I_{yy} = (1/4) m r^{2} [1 − sin(2θ)/(2θ)]'),
    I('I_{zz} = I_{xx} + I_{yy}')),
  body_group('Semicircular sector',pic_semi(fill=True),'A = (1/2) π r^{2}',
    I('y_{cm} = 4r/(3π)',note='from the flat edge'),
    I('I_{xx} = (1/4) m r^{2} (1 − 64/(9π^{2}))'),
    I('I_{yy} = (1/4) m r^{2}'),
    I('I_{zz} = (1/2) m r^{2} (1 − 32/(9π^{2}))')),
  body_group('Thin solid disk',pic_circle(fill=True),'A = π r^{2}',
    I('I_{xx} = I_{yy} = (1/4) m r^{2}'),
    I('I_{zz} = (1/2) m r^{2}'),
    I('x^{2} + y^{2} = r^{2}',note='Q = x b̂_{x} + y b̂_{y}')),
  body_group('Ellipse',pic_circle(fill=True,a=36,b=24),'A = π a b',
    I('I_{xx} = (1/4) m b^{2}'),
    I('I_{yy} = (1/4) m a^{2}'),
    I('I_{zz} = I_{xx} + I_{yy}'),
    I('x^{2}/a^{2} + y^{2}/b^{2} = 1',note='edge point Q')),
  body_group('Thin rectangular plate',pic_plate(),'A = a b',
    I('I_{xx} = (1/12) m b^{2}'),
    I('I_{yy} = (1/12) m a^{2}'),
    I('I_{zz} = I_{xx} + I_{yy}'))),
 S('Inertia of cylinders and cones',
  body_group('Solid cylinder',pic_cyl(),'V = π r^{2} h',
    I('I_{xx} = I_{yy} = (1/12) m (3r^{2} + h^{2})'),
    I('I_{zz} = (1/2) m r^{2}')),
  body_group('Thick cylindrical shell',pic_cyl('thick'),'V = π (r_{o}^{2} − r_{i}^{2}) h',
    I('I_{xx} = I_{yy} = (1/12) m [3(r_{o}^{2} + r_{i}^{2}) + h^{2}]'),
    I('I_{zz} = (1/2) m (r_{o}^{2} + r_{i}^{2})')),
  body_group('Thin cylindrical shell',pic_cyl('thin'),'A = 2π r h',
    I('I_{xx} = I_{yy} = (1/12) m (6r^{2} + h^{2})'),
    I('I_{zz} = m r^{2}')),
  body_group('Solid right-circular cone',pic_cone(),'V = (1/3) π r^{2} h',
    I('y_{cm} = h/4',note='from the base'),
    I('I_{xx} = I_{zz} = (3/80) m (4r^{2} + h^{2})'),
    I('I_{yy} = (3/10) m r^{2}')),
  body_group('Thin-walled cone (no base)',pic_cone(thin=True),'A = π r √(r^{2} + h^{2})',
    I('y_{cm} = h/3',note='from the base'),
    I('I_{xx} = I_{zz} = m (r^{2}/4 + h^{2}/18)'),
    I('I_{yy} = (1/2) m r^{2}')),
  body_group('Solid paraboloid of revolution',pic_parab(),'V = (1/2) π r^{2} a',
    I('x_{cm} = a/3',note='from the flat face, Bₒ'),
    I('I_{xx} = (1/3) m r^{2}',note='about the mass centre or Bₒ'),
    I('I_{yy} = I_{zz} = (1/18) m (a^{2} + 3r^{2})',note='about the mass centre'),
    I('I_{yy} = I_{zz} = (1/6) m (a^{2} + r^{2})',note='about Bₒ'))),
 S('Inertia of spheres and ellipsoids',
  body_group('Solid sphere',pic_sphere(),'V = (4/3) π r^{3}',
    I('I_{xx} = I_{yy} = I_{zz} = (2/5) m r^{2}')),
  body_group('Thin hollow sphere',pic_sphere(fill=False),'A = 4π r^{2}',
    I('I_{xx} = I_{yy} = I_{zz} = (2/3) m r^{2}')),
  body_group('Solid half sphere',pic_hemi(),'V = (2/3) π r^{3}',
    I('y_{cm} = 3r/8',note='above Bₒ'),
    I('I_{xx} = I_{yy} = I_{zz} = (2/5) m r^{2}',note='about Bₒ'),
    I('I_{xx} = I_{zz} = (83/320) m r^{2}',note='about the mass centre'),
    I('I_{yy} = (2/5) m r^{2}',note='about the mass centre')),
  body_group('Solid ellipsoid',pic_ellipsoid(),'V = (4/3) π a b c',
    I('I_{xx} = (1/5) m (b^{2} + c^{2})'),
    I('I_{yy} = (1/5) m (a^{2} + c^{2})'),
    I('I_{zz} = (1/5) m (a^{2} + b^{2})'),
    I('x^{2}/a^{2} + y^{2}/b^{2} + z^{2}/c^{2} = 1',note='surface point Q')),
  body_group('Thin-walled ellipsoid',pic_ellipsoid(fill=False),'A ≈ 4π [((ab)^{p} + (ac)^{p} + (bc)^{p})/3]^{1/p}',
    I('p = 1.6075'),
    I('I_{xx} = (1/3) m (b^{2} + c^{2})'),
    I('I_{yy} = (1/3) m (a^{2} + c^{2})'),
    I('I_{zz} = (1/3) m (a^{2} + b^{2})'),
    I('x^{2}/a^{2} + y^{2}/b^{2} + z^{2}/c^{2} = 1',note='surface point Q'))),
 S('Inertia of boxes',
  body_group('Solid box',pic_box(),'V = a b c',
    I('I_{xx} = (1/12) m (b^{2} + c^{2})'),
    I('I_{yy} = (1/12) m (a^{2} + c^{2})'),
    I('I_{zz} = (1/12) m (a^{2} + b^{2})')),
  body_group('Box of 8 particles',pic_box(particles=True),'m_{i} = m/8',
    I('I_{xx} = m (b^{2} + c^{2})',note='at (±a, ±b, ±c)'),
    I('I_{yy} = m (a^{2} + c^{2})'),
    I('I_{zz} = m (a^{2} + b^{2})'))),
 S('Inertia of a triangle and a tetrahedron',
  body_group('Triangle O P Q',pic_triangle(),'A = (1/2) |p⃗ × q⃗|',
    I('^{O}r⃗^{B_{cm}} = (p⃗ + q⃗)/3'),
    I('I⃡^{B∕O} = (m/6) [1⃡ (p⃗ · p⃗ + q⃗ · q⃗ + p⃗ · q⃗) − (p⃗ p⃗ + p⃗ q⃗/2 + q⃗ p⃗/2 + q⃗ q⃗)]'),
    I('I⃡^{B∕O} = (m/6) [d 1⃡ − (p⃗ p⃗_{sum} + q⃗ q⃗_{sum})]'),
    I('p⃗_{sum} = p⃗ + q⃗/2,  q⃗_{sum} = q⃗ + p⃗/2'),
    I('d = p⃗ · p⃗_{sum} + q⃗ · q⃗_{sum}')),
  G('Tetrahedron O P Q R',Pc(pic_tetra(),'V = (1/6) p⃗ × q⃗ · r⃗',note='= A h/3'),
    I('h = r⃗ · (p⃗ × q⃗)/(|p⃗ × q⃗|)',note='height above O P Q'),
    I('^{O}r⃗^{B_{cm}} = (p⃗ + q⃗ + r⃗)/4'),
    I('I⃡^{B∕O} = (m/10) [1⃡ (p⃗ · p⃗ + q⃗ · q⃗ + r⃗ · r⃗ + p⃗ · q⃗ + p⃗ · r⃗ + q⃗ · r⃗) − (p⃗ p⃗ + q⃗ q⃗ + r⃗ r⃗ + (p⃗ q⃗ + q⃗ p⃗ + p⃗ r⃗ + r⃗ p⃗ + q⃗ r⃗ + r⃗ q⃗)/2)]'),
    I('I⃡^{B∕O} = (m/10) [d 1⃡ − (p⃗ p⃗_{sum} + q⃗ q⃗_{sum} + r⃗ r⃗_{sum})]'),
    I('p⃗_{sum} = p⃗ + (q⃗ + r⃗)/2,  q⃗_{sum} = q⃗ + (p⃗ + r⃗)/2,  r⃗_{sum} = r⃗ + (p⃗ + q⃗)/2'),
    I('d = p⃗ · p⃗_{sum} + q⃗ · q⃗_{sum} + r⃗ · r⃗_{sum}')),
  wide=True),
 S('Momentum',
  G('Translational momentum',
    I('^{N}L⃗^{Q} = m^{Q} ^{N}v⃗^{Q}',note='particle'),
    I('^{N}L⃗^{S} = Σ_{i=1}^{n} m^{Q_{i}} ^{N}v⃗^{Q_{i}} = m^{S} ^{N}v⃗^{S_{cm}}',note='system')),
  G('Angular momentum',
    I('^{N}H⃗^{Q∕P} = ^{P}r⃗^{Q} × ^{N}L⃗^{Q}',note='particle Q about P'),
    I('^{N}H⃗^{B∕B_{p}} = I⃡^{B∕B_{p}} · ^{N}ω⃗^{B} + ^{B_{p}}r⃗^{B_{cm}} × m^{B} ^{N}v⃗^{B_{p}}',note='last term 0 if Bₚ is the mass centre or still'),
    I('^{N}H⃗^{B∕B_{p}} = I_{zz}^{B∕B_{p}} ^{N}ω⃗^{B} + ^{B_{p}}r⃗^{B_{cm}} × m^{B} ^{N}v⃗^{B_{p}}',note='ω⃗ = ω_{z} b̂_{z}'),
    I('^{N}H⃗^{S∕P} = ^{N}H⃗^{S∕O} + ^{P}r⃗^{O} × ^{N}L⃗^{S}',note='shift'),
    I('^{N}H⃗^{B∕P} = ^{N}H⃗^{B∕B_{p}} + ^{P}r⃗^{B_{p}} × ^{N}L⃗^{B}',note='shift, rigid body')),
  G('Impulse',
    I('∫ F⃗ dt = ΔL⃗',note='impulse-momentum'),
    I('∫ M⃗^{S∕O} dt = ΔH⃗^{S∕O}',note='O fixed, or the mass centre')),
  wide=True),
 S('Kinetic energy',
  G('In N',
    I('^{N}K^{Q} = (1/2) m^{Q} ^{N}v⃗^{Q} · ^{N}v⃗^{Q}',note='particle'),
    I('^{N}K^{B} = (1/2) m^{B} ^{N}v⃗^{B_{p}} · ^{N}v⃗^{B_{p}} + (1/2) ^{N}ω⃗^{B} · I⃡^{B∕B_{p}} · ^{N}ω⃗^{B} + m^{B} ^{N}v⃗^{B_{p}} · (^{N}ω⃗^{B} × ^{B_{p}}r⃗^{B_{cm}})',note='last term 0 at the mass centre'),
    I('^{N}K^{B} = (1/2) ^{N}v⃗^{B_{p}} · ^{N}L⃗^{B} + (1/2) ^{N}ω⃗^{B} · ^{N}H⃗^{B∕B_{p}}',note='any Bₚ fixed on B'),
    I('^{N}K^{B} = (1/2) m^{B} ^{N}v⃗^{B_{p}} · ^{N}v⃗^{B_{p}} + (1/2) I_{kk}^{B∕B_{p}} (^{N}ω^{B})^{2}',note='simple 2D; Bₚ the mass centre or fixed in N')),
  wide=True),
 S('Forces and moments',
  G('Forces',
    I('F⃗^{Q} = Σ_{i=1}^{n} F⃗^{Q∕P_{i}}',note='resultant on point Q'),
    I('F⃗^{S} = Σ_{i=1}^{n} F⃗^{Q_{i}}',note='resultant on a system'),
    I('F⃗^{P∕Q} = −F⃗^{Q∕P}',note='action and reaction'),
    I('F⃗^{S} = (F⃗^{S})_{external}',note='internal forces cancel')),
  G('Moments and torques',
    I('M⃗^{F⃗∕O} = ^{O}r⃗^{Q} × F⃗',note='force F⃗ at Q, about O'),
    I('M⃗^{S∕O} = Σ_{i=1}^{n} ^{O}r⃗^{Q_{i}} × F⃗^{Q_{i}}',note='a set of forces'),
    I('M⃗^{S∕O} = (M⃗^{S∕O})_{external}',note='internal moments cancel'),
    I('M⃗^{S∕P} = M⃗^{S∕O} + ^{P}r⃗^{O} × F⃗^{S}',note='shift'),
    I('M⃗ = T⃗',note='a couple: the same about any point')),
  G('Equivalent set',
    I('F⃗^{B_{p}} = F⃗^{S}',note='one force at Bₚ'),
    I('T⃗^{B} = M⃗^{S∕B_{p}}',note='plus one couple')),
  wide=True),
 S('Power, work and energy',
  G('In N',
    I('^{N}P = F⃗^{Q} · ^{N}v⃗^{Q}',note='a force on Q'),
    I('^{N}P^{S} = Σ_{i=1}^{n} F⃗^{Q_{i}} · ^{N}v⃗^{Q_{i}}',note='a set of forces'),
    I('^{N}P = T⃗^{B} · ^{N}ω⃗^{B}',note='a torque on B'),
    I('^{N}W^{S} = ∫ ^{N}P^{S} dt',note='work'),
    I('^{N}U^{S} = −^{N}W^{S}',note='if W depends on configuration only'),
    I('U = m g h',note='gravity, near the ground'),
    I('U = −(G m_{1} m_{2})/r',note='gravity'),
    I('U = (1/2) k x^{2}',note='spring')),
  wide=True),
 S('Equations of motion',
  G('Newton and Euler (N Newtonian)',
    I('F⃗^{Q} = m^{Q} ^{N}a⃗^{Q}',note='particle'),
    I('F⃗^{S} = m^{S} ^{N}a⃗^{S_{cm}}',note='system'),
    I('M⃗^{S∕O} = ^{N}d(^{N}H⃗^{S∕S_{cm}})/dt + ^{O}r⃗^{S_{cm}} × ^{N}d(^{N}L⃗^{S})/dt',note='angular momentum principle'),
    I('M⃗_{z}^{B∕P} = I_{zz}^{B∕B_{cm}} ^{N}α⃗^{B} + ^{P}r⃗^{B_{cm}} × m^{B} ^{N}a⃗^{B_{cm}}',note="Euler 2D, any P"),
    I('M⃗_{z}^{B∕B_{p}} = I_{zz}^{B∕B_{p}} ^{N}α⃗^{B} + ^{B_{p}}r⃗^{B_{cm}} × m^{B} ^{N}a⃗^{B_{p}}',note='Euler 2D, Bₚ on B'),
    I('M⃗^{B∕B_{p}} = I⃡^{B∕B_{p}} · ^{N}α⃗^{B} + ^{N}ω⃗^{B} × (I⃡^{B∕B_{p}} · ^{N}ω⃗^{B}) + ^{B_{p}}r⃗^{B_{cm}} × m^{B} ^{N}a⃗^{B_{p}}',note='Euler 3D'),
    I('M_{1} = I_{1} ω̇_{1} − (I_{2} − I_{3}) ω_{2} ω_{3}',note='principal axes; and cyclic'),
    I('Σ M_{O} = I_{O} α',note='2D, O fixed or the mass centre')),
  G('Energy and momentum',
    I('^{N}P^{S} = d(^{N}K^{S})/dt',note='power: rate of K'),
    I('W̃ = ΔK + ΔU',note='work of the forces not in U'),
    I('^{N}K^{S} + ^{N}U^{S} = constant',note='mechanical energy'),
    I('F⃗^{S} · û = m^{S} ^{N}a⃗^{S_{cm}} · û',note="D'Alembert: one direction û"),
    I('−∂U/∂q_{r} = d(∂K/∂q̇_{r})/dt − ∂K/∂q_{r}',note="Lagrange (2nd kind)")),
  wide=True),
 S('Solving a problem',
  G('Free-body diagram and road-map',
    W('Pick a system S that answers the question; draw its FBD'),
    W('Draw only external forces on S: contact and distance (gravity)'),
    W('Replace a messy set of forces with one force at a point and a couple'),
    W('Name points, unit vectors and force and torque measures as needed'),
    W('Use action and reaction to cut down the unknowns'),
    W('Motion limited along û at Q: there is a force on Q along û'),
    W('Rotation limited about û: there is a torque on B about û'),
    W('Road-map: variable, translate or rotate, û, S, FBD, about point'),
    W('Then add constraint equations and their derivatives (linkages, rolling)'),
    W('Momentum is conserved along û if no external force (or moment) acts there')),
  wide=True),
 S('Statics',
  G('Equilibrium',
    I('Σ F = 0',note='equilibrium: the forces balance'),
    I('Σ M = 0',note='equilibrium: moments about any point balance'),
    I('M = F d',note='moment: force × perpendicular distance'),
    I('f ≤ μ N',note='dry (Coulomb) friction'),
    I('tan(φ) = μ',note='angle of friction, angle of repose'),
    I('T_{2} = T_{1} e^{μβ}',note='capstan: T₂ tight side, β in rad')),
  G('Centroids',
    I('x̄ = (Σ A_{i} x_{i})/(Σ A_{i})',note='centroid of a composite area'),
    I('x̄ = (∫ x dA)/(∫ dA)',note='centroid by integration'),
    I('I = Ī + A d^{2}',note='parallel axis'))),
 S('Motion',
  G('Particles',
    I('a = v dv/ds'),
    I('a_{n} = v^{2}/ρ',note='normal, radius ρ'),
    I('W = Δ KE',note='work-energy theorem'),
    I('P = dW/dt = F v',note='power'),
    I('∫ F dt = Δp',note='impulse-momentum'),
    I('m_{1} v_{1} + m_{2} v_{2} = m_{1} v′_{1} + m_{2} v′_{2}',note='momentum conserved'),
    I('e = (v′_{2} − v′_{1})/(v_{1} − v_{2})',note='restitution: 1 elastic, 0 plastic')),
  G('Rotation and gears',
    I('v = ω r',note='rolling without slipping'),
    I('Σ M = I α',note='rotation about a fixed axis'),
    I('ω_{1}/ω_{2} = N_{2}/N_{1}',note='gear ratio: speeds, N teeth'),
    I('τ_{2}/τ_{1} = N_{2}/N_{1}',note='gear ratio: torques'))),
 S('Vibrations',
  G('Mass, spring and damper',
    I('m x′′ + c x′ + k x = 0',note='free vibration, mass-spring-damper'),
    I('k_{eq} = k_{1} + k_{2}',note='springs in parallel'),
    I('1/k_{eq} = 1/k_{1} + 1/k_{2}',note='springs in series'),
    I('ω_{n} = √(k/m)',note='natural frequency'),
    I('f_{n} = ω_{n}/(2π)',note='natural frequency in hertz'),
    I('c_{c} = 2√(k m)',note='critical damping'),
    I('ζ = c/c_{c}',note='damping ratio'),
    I('ω_{d} = ω_{n} √(1 − ζ^{2})',note='damped natural frequency'),
    I('x = A e^{−ζω_{n}t} cos(ω_{d}t − φ)',note='underdamped free response'),
    I('δ = ln(x_{1}/x_{2})',note='logarithmic decrement'),
    I('ζ = δ/√(4π^{2} + δ^{2})',note='damping ratio from the log decrement'),
    I('ω_{n} = √(g/L)',note='simple pendulum'),
    I('ω_{n} = √(m g d/I_{O})',note='physical pendulum, pivot O')),
  G('Forced vibration, r = ω/ω_n',
    I('m x′′ + c x′ + k x = F_{0} cos(ωt)',note='harmonic forcing'),
    I('X = (F_{0}/k)/√((1 − r^{2})^{2} + (2ζr)^{2})',note='steady-state amplitude, magnification factor'),
    I('tan(φ) = 2ζr/(1 − r^{2})',note='phase lag'),
    I('TR = √(1 + (2ζr)^{2})/√((1 − r^{2})^{2} + (2ζr)^{2})',note='transmissibility, vibration isolation')),
  wide=True),
 S('Constants',
  G('Mitiguy, p. 306',
    H('Quantity','Value'),
    U('Standard g','9.80665 m/s^{2}'),
    U('G','6.67 × 10^{−11} N m^{2}/kg^{2}'),
    U('Speed of light','299792458 m/s'),
    U('Mass of the Earth','5.9742 × 10^{24} kg'),
    U('Mass of the Moon','7.34 × 10^{22} kg'),
    U('Earth to Moon','384400 km'),
    U('Earth to Sun (mean)','149597870 km'),
    U('Perihelion, aphelion','147085800, 152104980 km'),
    U('Earth radius (equatorial)','6378 km'),
    U('Earth diameters','12712.02 polar, 12756.28 equatorial km'),
    U('Earth surface area','5.10 × 10^{8} km^{2}'),
    U('Earth volume','1.0832 × 10^{12} km^{3}'),
    U('Earth density','5.515 g/cm^{3}'),
    U('Earth escape velocity','11.18 km/s'),
    U('Earth orbital velocity','107244 km/hr'),
    U('Earth axial tilt','23.45°'),
    U('Sidereal day','23 hr 56.1 min'),
    U('Sidereal year','365.2422 days'),
    U('Geosynchronous radius','4.2108 × 10^{7} m')),
  G('Little g and radius',
    H('Body','g (m/s²)','Radius (km)'),
    *[L(n,g,r) for n,g,r in [('Sun','274.0','695500'),('Jupiter','20.9','71492'),('Neptune','10.7','24764'),('Saturn','10.4','60268'),('Earth','9.8','6378'),('Venus','8.9','6051'),('Uranus','8.4','25559'),('Mars','3.7','3397'),('Mercury','3.7','2439'),('Moon','1.6','1737'),('Pluto','0.8','1151')]]),
  wide=True,leads=[180,90]))

heat=SH('heat transfer','Heat Transfer','heat transfer: conduction, resistances, fins, transient, convection, exchangers, radiation',
 S('Modes',
  G('Rates',
    I('Q̇ = −k A dT/dx',note="conduction, Fourier's law"),
    I('Q̇ = k A (T_{1} − T_{2})/L',note='plane wall, steady conduction'),
    I('q″ = Q̇/A',note='heat flux'),
    I('Q̇ = h A (T − T_{∞})',note="convection, Newton's law of cooling"),
    I('Q̇ = ε σ A (T_{1}^{4} − T_{2}^{4})',note='radiation to large surroundings'))),
 S('Thermal resistance',
  G('Q̇ = ΔT/R, like Ohm\'s law',
    I('Q̇ = ΔT/R',note='thermal circuit'),
    I('R = L/(k A)',note='plane wall, conduction'),
    I('R = ln(r_{2}/r_{1})/(2π k L)',note='cylinder (pipe) wall'),
    I('R = (1/r_{1} − 1/r_{2})/(4π k)',note='spherical shell'),
    I('R = 1/(h A)',note='convection'),
    I('1/(U A) = Σ R',note='resistances in series, overall U'),
    I('r_{cr} = k/h',note='critical insulation radius, cylinder'))),
 S('Conduction',
  G('Transient',
    I('α = k/(ρ c_{p})',note='thermal diffusivity'),
    I('Bi = h L/k',note='Biot number, L = V/A; lumped if under 0.1'),
    I('Fo = α t/L^{2}',note='Fourier number, dimensionless time'),
    I('T − T_{∞} = (T_{0} − T_{∞}) e^{−t/τ}',note='lumped capacitance cooling or heating'),
    I('τ = ρ V c_{p}/(h A)',note='lumped time constant')),
  G('Fins',
    I('m = √(h P/(k A_{c}))',note='fin parameter: P perimeter, A_c cross section'),
    I('Q̇ = √(h P k A_{c}) (T_{b} − T_{∞}) tanh(m L)',note='fin heat rate (heat loss), adiabatic tip'),
    I('L_{c} = L + A_{c}/P',note='corrected length for a convective tip: L + t/2 plate, L + D/4 pin'),
    I('Q̇ = √(h P k A_{c}) (T_{b} − T_{∞})',note='infinitely long fin'),
    I('η_{f} = tanh(m L)/(m L)',note='fin efficiency, adiabatic tip'),
    I('Q̇ = η_{f} h A_{f} (T_{b} − T_{∞})',note='fin heat rate from its efficiency, A_f fin surface'),
    I('ε_{fin} = Q̇_{fin}/(h A_{c} (T_{b} − T_{∞}))',note='fin effectiveness: worth adding when well above 2')),
  G('Heat generation',
    I('T(x) = T_{s} + (q̇ L^{2}/(2k))(1 − x^{2}/L^{2})',note='plane wall of half-thickness L, uniform heat generation q̇'),
    I('T(r) = T_{s} + (q̇ r_{o}^{2}/(4k))(1 − r^{2}/r_{o}^{2})',note='solid cylinder or wire with heat generation q̇')),
  wide=True),
 S('Convection',
  G('Dimensionless numbers',
    I('Nu = h L/k',note='Nusselt: convection over conduction'),
    I('Re = ρ v L/μ',note='Reynolds: inertia over viscosity'),
    I('Pr = μ c_{p}/k',note='Prandtl: momentum over heat diffusion'),
    I('Gr = g β (T_{s} − T_{∞}) L^{3}/ν^{2}',note='Grashof: buoyancy over viscosity, natural (free) convection'),
    I('Ra = Gr Pr',note='Rayleigh: sets the natural convection regime')),
  G('Correlations',
    I('Nu = 3.66',note='laminar pipe, constant wall temperature'),
    I('Nu = 4.36',note='laminar pipe, constant heat flux'),
    I('Nu = 0.023 Re^{0.8} Pr^{n}',note='Dittus-Boelter, turbulent pipe: n = 0.4 heating, 0.3 cooling'),
    I('Nu = 0.664 Re^{1/2} Pr^{1/3}',note='laminar flat plate, average'),
    I('Nu = 0.037 Re^{4/5} Pr^{1/3}',note='turbulent flat plate, average'),
    I('Nu = C Ra^{n}',note='natural convection: vertical plate C = 0.59, n = 1/4 for Ra 10⁴–10⁹'),
    I('(T_{s} − T_{out})/(T_{s} − T_{in}) = e^{−h P L/(ṁ c_{p})}',note='pipe outlet temperature, constant wall temperature')),
  wide=True),
 S('Heat exchangers',
  G('LMTD method',
    I('Q̇ = ṁ c_{p} (T_{out} − T_{in})',note='energy balance on one stream'),
    I('Q̇ = U A ΔT_{m}',note='LMTD method'),
    I('ΔT_{m} = (ΔT_{1} − ΔT_{2})/ln(ΔT_{1}/ΔT_{2})',note='log mean temperature difference (LMTD)'),
    I('ΔT_{1} = T_{h,in} − T_{c,out},  ΔT_{2} = T_{h,out} − T_{c,in}',note='counterflow: the two ends'),
    I('ΔT_{1} = T_{h,in} − T_{c,in},  ΔT_{2} = T_{h,out} − T_{c,out}',note='parallel flow: the two ends'),
    I('Q̇ = U A F ΔT_{m,cf}',note='shell-and-tube or cross-flow: correction factor F on the counterflow LMTD')),
  G('Effectiveness-NTU',
    I('NTU = U A/C_{min}',note='number of transfer units, C = ṁ c_p'),
    I('Q̇_{max} = C_{min} (T_{h,in} − T_{c,in})',note='most heat any exchanger could pass'),
    I('ε = Q̇/Q̇_{max}',note='effectiveness'),
    I('ε = (1 − e^{−NTU(1 − c)})/(1 − c e^{−NTU(1 − c)})',note='counterflow, c = C_min/C_max'),
    I('ε = (1 − e^{−NTU(1 + c)})/(1 + c)',note='parallel flow, capacity ratio c'),
    I('ε = 1 − e^{−NTU}',note='c = 0: a boiling or condensing stream, any exchanger')),
  wide=True),
 S('Radiation',
  G('Surfaces',
    I('E_{b} = σ T^{4}',note='blackbody emissive power, Stefan-Boltzmann'),
    I('λ_{max} = b/T',note="Wien's displacement law, b = 2898 μm K"),
    I('α + ρ + τ = 1',note='absorbed, reflected, transmitted'),
    I('ε = α',note="Kirchhoff's law (gray surface)"),
    I('A_{1} F_{12} = A_{2} F_{21}',note='view factor reciprocity'),
    I('Σ_{j} F_{ij} = 1',note='view factor summation rule')),
  G('Exchange between gray surfaces',
    I('Q̇_{12} = σ (T_{1}^{4} − T_{2}^{4})/((1 − ε_{1})/(A_{1} ε_{1}) + 1/(A_{1} F_{12}) + (1 − ε_{2})/(A_{2} ε_{2}))',note='two-surface enclosure: radiation between two surfaces'),
    I('q″ = σ (T_{1}^{4} − T_{2}^{4})/(1/ε_{1} + 1/ε_{2} − 1)',note='large parallel plates')),
  wide=True))

compsci=SH('computer science','Computer Science','computer science: complexity, recurrences, hashing, trees and graphs, bits, performance, networks',
 S('Algorithms',
  G('Sums and bounds',
    I('Σ_{i=1}^{n} i = n(n + 1)/2',note='arithmetic series: nested loops are O(n²)'),
    I('Σ_{i=0}^{n} 2^{i} = 2^{n+1} − 1',note='geometric series: doubling'),
    I('Σ_{i=1}^{n} 1/i ≈ ln(n) + 0.5772',note='harmonic series: quicksort average, coupon collector'),
    I('log_{2}(n!) ≈ n log_{2}(n)',note='comparison sorting lower bound, Stirling')),
  G('Recurrences: master theorem',
    I('T(n) = a T(n/b) + f(n)',note='master theorem, divide and conquer'),
    I('T(n) = Θ(n^{log_{b} a})',note='case 1: f smaller, the leaves dominate'),
    I('T(n) = Θ(n^{log_{b} a} log n)',note='case 2: every level costs the same'),
    I('T(n) = Θ(f(n))',note='case 3: f larger, the root dominates (if regular)'),
    I('T(n) = 2T(n/2) + Θ(n) = Θ(n log n)',note='merge sort'),
    I('T(n) = T(n/2) + Θ(1) = Θ(log n)',note='binary search')),
  G('Hashing',
    I('α = n/m',note='load factor: n keys, m slots'),
    I('E = 1 + α',note='expected cost of a search with chaining'),
    I('E = 1/(1 − α)',note='expected probes, open addressing, unsuccessful search'),
    I('P ≈ 1 − e^{−n^{2}/(2m)}',note='birthday problem: chance of any collision')),
  wide=True),
 S('Trees and graphs',
  G('Counts',
    I('n = 2^{h+1} − 1',note='nodes in a perfect binary tree of height h'),
    I('E = V − 1',note='edges in a tree'),
    I('E = V(V − 1)/2',note='edges in a complete graph'),
    I('Σ deg(v) = 2E',note='handshake lemma'),
    I('V − E + F = 2',note="Euler's formula for a planar graph")),
  wide=True),
 S('Information',
  G('Bits',
    I('N = 2^{n}',note='values in n bits'),
    I('n = log_{2}(N)',note='bits needed, rounded up'),
    I('0 ≤ x ≤ 2^{n} − 1',note='unsigned integer range, n bits'),
    I('−2^{n−1} ≤ x ≤ 2^{n−1} − 1',note="two's complement (signed) range, n bits"),
    I('H = −Σ p_{i} log_{2}(p_{i})',note='Shannon entropy, bits per symbol')),
  wide=True),
 S('Systems',
  G('Performance',
    I('S = 1/((1 − p) + p/s)',note="Amdahl's law: speedup when a fraction p runs s times faster"),
    I('S = (1 − p) + p N',note="Gustafson's law: scaled speedup on N processors"),
    I('t = N CPI/f',note='CPU time: instructions × cycles per instruction / clock rate'),
    I('MIPS = f/(CPI × 10^{6})',note='millions of instructions per second'),
    I('L = λ W',note="Little's law: items in system = arrival rate × time in system")),
  G('Caches',
    I('AMAT = t_{hit} + m t_{miss}',note='average memory access time, miss rate m'),
    I('S = C/(B k)',note='number of sets: capacity C, block size B, k-way associative'),
    I('t = a − log_{2}(S) − log_{2}(B)',note='tag bits of an a-bit address: index log₂ S bits, offset log₂ B bits')),
  G('Networks',
    I('t = L/R',note='transmission delay: L bits at R bit/s'),
    I('t = d/s',note='propagation delay: distance over signal speed'),
    I('BDP = R RTT',note='bandwidth-delay product: bits in flight'),
    I('U = (L/R)/(RTT + L/R)',note='stop-and-wait link utilization'),
    I('C = B log_{2}(1 + S/N)',note='Shannon capacity, max bit rate of a channel'),
    I('d = d_{proc} + d_{queue} + d_{trans} + d_{prop}',note='total delay at one hop')),
  wide=True))

mlsheet=SH('machine learning','Machine Learning','machine learning: regression, activations, attention, losses, training, metrics',
 S('Models',
  G('Regression and classification',
    I('ŷ = w · x + b',note='linear regression model'),
    I('w = (X^{T} X)^{−1} X^{T} y',note='normal equation, ordinary least squares'),
    I('w = (X^{T} X + λI)^{−1} X^{T} y',note='ridge regression, closed form'),
    I('ŷ = σ(w · x + b)',note='logistic regression'),
    I('ŷ = argmax_{y} P(y) Π_{i} P(x_{i} | y)',note='naive Bayes classifier'),
    I('margin = 2/‖w‖',note='SVM margin, support vector machine'),
    I('d(x, y) = √(Σ (x_{i} − y_{i})^{2})',note='Euclidean distance, k-nearest neighbours, k-means'),
    I('cos(θ) = (a · b)/(|a| |b|)',note='cosine similarity, embeddings')),
  G('Clustering, PCA and decision trees',
    I('J = Σ_{i} ‖x_{i} − μ_{c_{i}}‖^{2}',note='k-means objective: squared distance to each point\'s centroid'),
    I('λ_{k}/(Σ_{i} λ_{i})',note='PCA: share of variance explained by component k'),
    I('G = 1 − Σ p_{i}^{2}',note='Gini impurity, decision trees'),
    I('H = −Σ p_{i} log_{2}(p_{i})',note='entropy, decision trees'),
    I('IG = H(parent) − Σ (n_{k}/n) H(child_{k})',note='information gain, decision trees')),
  G('Neural networks: activations, attention, convolution',
    I('σ(z) = 1/(1 + e^{−z})',note='sigmoid (logistic) function'),
    I('σ′(z) = σ(z)(1 − σ(z))',note='derivative of the sigmoid'),
    I('tanh(z) = (e^{z} − e^{−z})/(e^{z} + e^{−z})',note='hyperbolic tangent'),
    I('ReLU(z) = max(0, z)',note='ReLU, rectified linear unit'),
    I('p_{i} = e^{z_{i}}/(Σ_{j} e^{z_{j}})',note='softmax: scores to probabilities'),
    I('A = softmax(Q K^{T}/√(d_{k})) V',note='scaled dot-product attention, transformers'),
    I('n_{out} = (n + 2p − k)/s + 1',note='convolution output size: kernel k, padding p, stride s; round down')),
  wide=True),
 S('Losses',
  G('Regression',
    I('MSE = (1/n) Σ (y_{i} − ŷ_{i})^{2}',note='mean squared error'),
    I('RMSE = √(MSE)',note='root mean squared error'),
    I('MAE = (1/n) Σ |y_{i} − ŷ_{i}|',note='mean absolute error'),
    I('R^{2} = 1 − SS_{res}/SS_{tot}',note='coefficient of determination: SS_res = Σ (yᵢ − ŷᵢ)², SS_tot = Σ (yᵢ − ȳ)²'),
    I('E[(y − ŷ)^{2}] = Bias^{2} + Var + σ^{2}',note='bias-variance decomposition: underfitting vs overfitting')),
  G('Classification',
    I('L = −Σ y_{i} ln(ŷ_{i})',note='cross-entropy (categorical), y one-hot'),
    I('L = −(y ln(ŷ) + (1 − y) ln(1 − ŷ))',note='binary cross-entropy, log loss'),
    I('L = max(0, 1 − y ŷ)',note='hinge loss, SVM, y = ±1'),
    I('D = Σ p_{i} ln(p_{i}/q_{i})',note='KL divergence')),
  G('Regularization',
    I('L + λ ‖w‖^{2}',note='L2 regularization (ridge, weight decay)'),
    I('L + λ ‖w‖_{1}',note='L1 regularization (lasso), sparse weights')),
  wide=True),
 S('Training',
  G('Gradients and optimizers',
    I('w ← w − η ∇L',note='gradient descent, learning rate η'),
    I('∇_{w} L = (ŷ − y) x',note='gradient for one example: logistic regression with log loss, or linear with (1/2) squared error'),
    I('v ← β v + ∇L,  w ← w − η v',note='gradient descent with momentum'),
    I('w ← w − η m̂/(√(v̂) + ϵ)',note='Adam: m̂, v̂ bias-corrected averages of the gradient and its square')),
  G('Feature scaling',
    I('z = (x − μ)/σ',note='standardization (z-score)'),
    I('x′ = (x − x_{min})/(x_{max} − x_{min})',note='min-max normalization to [0, 1]')),
  wide=True),
 S('Scoring',
  G('From the confusion matrix',
    I('P = TP/(TP + FP)',note='precision: of those predicted positive, the share right'),
    I('R = TP/(TP + FN)',note='recall, sensitivity, true positive rate (TPR)'),
    I('TNR = TN/(TN + FP)',note='specificity, true negative rate'),
    I('FPR = FP/(FP + TN)',note='false positive rate, the ROC curve\'s x-axis'),
    I('F_{1} = 2PR/(P + R)',note='F1 score, harmonic mean of precision and recall'),
    I('A = (TP + TN)/(TP + TN + FP + FN)',note='accuracy')),
  wide=True))

astronomy=SH('astronomy','Astronomy','astronomy: orbits, gravity, magnitudes, stars, distance, cosmology, telescopes',
 S('Orbits and gravity',
  G('Orbits',
    I('T^{2} = 4π^{2} a^{3}/(G M)',note="Kepler's third law: orbit period"),
    I('P^{2} = a^{3}',note="Kepler's third law, years and AU"),
    I('T^{2} = 4π^{2} a^{3}/(G (M_{1} + M_{2}))',note='binary stars: their total mass'),
    I('v = √(G M/r)',note='circular orbit speed'),
    I('v_{e} = √(2 G M/r)',note='escape velocity'),
    I('v^{2} = G M (2/r − 1/a)',note='vis-viva: speed on an ellipse'),
    I('r_{p} = a(1 − e),  r_{a} = a(1 + e)',note='periapsis, apoapsis'),
    I('E = −G M m/(2a)',note='orbital energy (total), bound orbit'),
    I('1/S = 1/P_{1} − 1/P_{2}',note='synodic period: P₁ the inner (faster) orbit')),
  G('Gravity',
    I('g = G M/r^{2}',note='surface gravity'),
    I('r_{s} = 2 G M/c^{2}',note='Schwarzschild radius, black hole'))),
 S('Light and stars',
  G('Light',
    I('L = 4π R^{2} σ T^{4}',note='luminosity of a star, Stefan-Boltzmann'),
    I('λ_{max} = b/T',note="Wien's law, b = 2.898 mm K"),
    I('F = L/(4π d^{2})',note='flux (brightness), inverse square law'),
    I('m_{1} − m_{2} = −2.5 log_{10}(F_{1}/F_{2})',note='magnitude scale (Pogson)'),
    I('m − M = 5 log_{10}(d) − 5',note='distance modulus, d in parsecs'),
    I('M_{bol} = 4.74 − 2.5 log_{10}(L/L_{sun})',note='absolute magnitude')),
  G('Stars',
    I('L/L_{sun} = (M/M_{sun})^{3.5}',note='mass-luminosity, main sequence'),
    I('t ≈ 10^{10} yr (M/M_{sun})^{−2.5}',note='main-sequence lifetime'))),
 S('Distance and cosmology',
  G('Distance',
    I('d = 1/p',note='parallax: p in arcseconds, d in parsecs'),
    I('θ = D/d',note='small-angle formula, θ in radians')),
  G('Expansion',
    I('v = H_{0} d',note="Hubble's law"),
    I('t_{H} = 1/H_{0}',note='Hubble time, rough age of the universe'),
    I('z = Δλ/λ',note='redshift'),
    I('v = c z',note='recession speed from redshift, v ≪ c'),
    I('1 + z = λ_{obs}/λ_{emit}',note='redshift as stretch of wavelength'))),
 S('Telescopes',
  G('Optics',
    I('θ = 1.22 λ/D',note='Rayleigh criterion (diffraction limit)'),
    I('M = f_{o}/f_{e}',note='telescope magnification'),
    I('P_{1}/P_{2} = (D_{1}/D_{2})^{2}',note='light-gathering power by aperture'))))

finance=SH('finance','Finance Formulas','finance formulas: interest, present and future value, annuities, loans, investing, business',
 S('Interest and growth',
  G('Growth',
    I('A = P(1 + r t)',note='simple interest'),
    I('A = P(1 + r/n)^{n t}',note='compound interest, n times a year'),
    I('A = P e^{r t}',note='continuous compounding'),
    I('EAR = (1 + r/n)^{n} − 1',note='effective annual rate (APY)'),
    I('CAGR = (V_{f}/V_{i})^{1/n} − 1',note='compound annual growth rate'),
    I('t ≈ 72/r',note='rule of 72: doubling time, r in percent'),
    I('r_{real} = (1 + r)/(1 + i) − 1',note='real rate after inflation i (Fisher)')),
  wide=True),
 S('Time value of money',
  G('Single sums (r per period, n periods)',
    I('PV = FV/(1 + r)^{n}',note='present value, discounting'),
    I('FV = PV (1 + r)^{n}',note='future value'),
    I('PV = FV e^{−r t}',note='continuous discounting')),
  G('Streams of payments',
    I('FV = PMT ((1 + r)^{n} − 1)/r',note='future value of an annuity'),
    I('PV = PMT (1 − (1 + r)^{−n})/r',note='present value of an annuity'),
    I('PV_{due} = (1 + r) PMT (1 − (1 + r)^{−n})/r',note='annuity due: payments at the start of each period'),
    I('M = P r/(1 − (1 + r)^{−n})',note='loan or mortgage payment'),
    I('B = P (1 + r)^{k} − M ((1 + r)^{k} − 1)/r',note='balance after k payments'),
    I('PV = C/r',note='perpetuity'),
    I('PV = C/(r − g)',note='growing perpetuity (Gordon), r > g')),
  G('Judging a project (capital budgeting)',
    I('NPV = Σ C_{t}/(1 + r)^{t}',note='net present value: take the project if NPV > 0'),
    I('Σ C_{t}/(1 + IRR)^{t} = 0',note='internal rate of return: the rate where NPV = 0'),
    I('PP = I/C',note='payback period: initial cost over the yearly cash flow')),
  wide=True),
 S('Investing',
  G('Return and risk',
    I('ROI = gain/cost',note='return on investment; gain is the net profit'),
    I('E[R] = r_{f} + β (E[R_{m}] − r_{f})',note='CAPM: expected return'),
    I('S = (R_{p} − r_{f})/σ_{p}',note='Sharpe ratio'),
    I('WACC = (E/V) r_{E} + (D/V) r_{D} (1 − T)',note='cost of capital')),
  G('Portfolios and bonds',
    I('E[R_{p}] = Σ w_{i} E[R_{i}]',note='portfolio expected return, weights w'),
    I('σ_{p}^{2} = w_{1}^{2}σ_{1}^{2} + w_{2}^{2}σ_{2}^{2} + 2w_{1}w_{2}ρσ_{1}σ_{2}',note='two-asset portfolio variance, correlation ρ'),
    I('P = Σ_{t=1}^{n} C/(1 + y)^{t} + F/(1 + y)^{n}',note='bond price, yield to maturity y'),
    I('CY = C/P',note='current yield: annual coupon over price')),
  wide=True),
 S('Business',
  G('Break-even, margins and depreciation',
    I('Q = F/(p − v)',note='break-even units'),
    I('PM = (R − C)/R',note='profit margin: revenue R, cost C'),
    I('D = (C − S)/n',note='straight-line depreciation'),
    I('B_{k} = C (1 − d)^{k}',note='declining-balance book value; double-declining d = 2/n')),
  wide=True))

# round 4 exam walkthrough: geometry, statistics, finance, computer science, machine learning (what a student types)
TAGS=globals().get('TAGS',{})
for k,v in {
 'c = 2r sin(θ/2)':'chord of a circle subtended central angle',
 'A = π(R^{2} − r^{2})':'annulus ring washer hollow circle area between two circles',
 'A = (1/2) d_{1} d_{2}':'rhombus kite diagonals area',
 'V = (1/3) πh(R^{2} + R r + r^{2})':'frustum truncated cone volume bucket',
 '360°/n':'exterior angle regular polygon',
 'A_{2}/A_{1} = k^{2},  V_{2}/V_{1} = k^{3}':'similar triangles similarity scale factor ratio of areas ratio of volumes',
 'θ_{i} = θ_{c}/2':'inscribed angle theorem central angle circle theorem angle at the centre',
 'A = πr^{2}':'area of a circle',
 'C = 2πr':'circumference perimeter of a circle 2 pi r',
 'A = (1/2) r^{2} θ':'sector area pie slice',
 's = rθ':'arc length',
 'V = (4/3) πr^{3}':'volume of a sphere ball',
 'S = 4πr^{2}':'surface area of a sphere',
 'V = πr^{2} h':'volume of a cylinder',
 'V = (1/3) πr^{2} h':'volume of a cone',
 'A = (1/2) b h':'area of a triangle half base times height',
 'A + B + C = 180°':'triangle angles add to 180 degrees',
 'AMAT = t_{hit} + m t_{miss}':'cache memory access hit miss penalty',
 'IQR = Q_{3} − Q_{1}':'interquartile range quartiles outlier fence box plot five number summary',
 'z = (p̂ − p_{0})/√(p_{0}(1 − p_{0})/n)':'one proportion z test hypothesis test proportion',
 't = (x̄_{1} − x̄_{2})/√(s_{1}^{2}/n_{1} + s_{2}^{2}/n_{2})':'two sample t test welch difference of means compare two groups independent samples',
 't = (x̄ − μ_{0})/(s/√n)':'one sample t test hypothesis test t statistic',
 'z = (x̄ − μ_{0})/(σ/√n)':'one sample z test hypothesis test z statistic',
 'χ^{2} = Σ (O − E)^{2}/E':'chi square test degrees of freedom contingency',
 'within 1σ: 68.27%':'empirical rule 68 95 99.7 normal distribution one standard deviation',
 'within 2σ: 95.45%':'empirical rule 68 95 99.7 two standard deviations',
 'within 3σ: 99.73%':'empirical rule 68 95 99.7 three standard deviations',
 'z = 1.960':'critical value z star 95 percent confidence',
 'x̄ ± t s/√n':'t interval confidence interval mean unknown sigma',
 'x̄ ± z σ/√n':'z interval confidence interval mean',
 'Binomial ‖ nCk p^{k} (1 − p)^{n−k} ‖ np, np(1 − p)':'binomial probability n trials k successes',
 'Σ_{i=1}^{n} 1/i ≈ ln(n) + 0.5772':'harmonic number euler mascheroni gamma sum of reciprocals',
 'E = 1 + α':'separate chaining linked list hash table search time',
 'S = C/(B k)':'cache sets associativity direct mapped set associative block size cache organization',
 't = a − log_{2}(S) − log_{2}(B)':'cache tag bits index bits offset bits address breakdown',
 'MIPS = f/(CPI × 10^{6})':'mips instruction rate clock rate',
 'U = (L/R)/(RTT + L/R)':'stop and wait utilization efficiency sender utilization pipelining arq',
 'n = 2^{h+1} − 1':'height of a binary tree log n full binary tree',
 '−2^{n−1} ≤ x ≤ 2^{n−1} − 1':'signed integer overflow negate flip the bits add one int range',
 'ŷ = argmax_{y} P(y) Π_{i} P(x_{i} | y)':'naive bayes classifier conditional independence posterior prior likelihood',
 'margin = 2/‖w‖':'svm support vector machine maximum margin hyperplane',
 'J = Σ_{i} ‖x_{i} − μ_{c_{i}}‖^{2}':'k means clustering inertia within cluster sum of squares wcss centroid elbow method',
 'λ_{k}/(Σ_{i} λ_{i})':'pca principal component analysis explained variance ratio eigenvalues dimensionality reduction',
 'n_{out} = (n + 2p − k)/s + 1':'cnn convolution layer output size feature map padding stride kernel filter',
 'E[(y − ŷ)^{2}] = Bias^{2} + Var + σ^{2}':'bias variance tradeoff overfitting underfitting irreducible error',
 '∇_{w} L = (ŷ − y) x':'gradient of the loss backpropagation derivative of log loss logistic regression gradient',
 'G = 1 − Σ p_{i}^{2}':'gini index decision tree split',
 'IG = H(parent) − Σ (n_{k}/n) H(child_{k})':'decision tree split entropy id3',
 'PV_{due} = (1 + r) PMT (1 − (1 + r)^{−n})/r':'annuity due payments in advance beginning of period',
 'Σ C_{t}/(1 + IRR)^{t} = 0':'irr internal rate of return yield project rate',
 'PP = I/C':'payback period capital budgeting break even time',
 'E[R_{p}] = Σ w_{i} E[R_{i}]':'portfolio return weighted average expected return',
 'σ_{p}^{2} = w_{1}^{2}σ_{1}^{2} + w_{2}^{2}σ_{2}^{2} + 2w_{1}w_{2}ρσ_{1}σ_{2}':'portfolio variance risk diversification correlation two stocks',
 'CY = C/P':'current yield bond coupon',
 'P = Σ_{t=1}^{n} C/(1 + y)^{t} + F/(1 + y)^{n}':'bond valuation ytm yield to maturity coupon bond',
 'B_{k} = C (1 − d)^{k}':'declining balance double declining ddb depreciation book value',
 'Q = F/(p − v)':'break even point contribution margin',
 'PV = PMT (1 − (1 + r)^{−n})/r':'ordinary annuity present value of payments',
 'A = P(1 + r t)':'simple interest i = prt',
}.items():
    TAGS[k]=(TAGS.get(k,'')+' '+v).strip()

# search tags for precalculus to continuum mechanics and statics and dynamics: the words a student types that a row's
# note doesn't already hold (acronyms, other names, what it's used for). Merged into TAGS, never replacing it.
TAGS=globals().get('TAGS',{})
for k,v in {
 # precalculus
 'x = (−b ± √(b^{2} − 4ac))/(2a)':'roots zeros solve quadratic equation abc formula',
 'Δ = b^{2} − 4ac':'nature of roots real complex',
 'x = −b/(2a)':'vertex turning point maximum minimum parabola',
 'y = a(x − h)^{2} + k':'parabola standard form turning point',
 'm = (y_{2} − y_{1})/(x_{2} − x_{1})':'gradient rise run',
 'y = m x + b':'equation of a line y = mx + c gradient intercept',
 'y − y_{1} = m(x − x_{1})':'equation of a line through a point',
 'm_{1} m_{2} = −1':'normal line negative reciprocal gradient parallel',
 '(a + b)^{n} = Σ_{k=0}^{n} C(n, k) a^{n−k}b^{k}':'pascal triangle expansion choose ncr',
 'log_{b}(x) = ln(x)/ln(b)':'logarithm convert base',
 'log_{b}(xy) = log_{b}(x) + log_{b}(y)':'logarithm laws log rules',
 'log_{b}(x/y) = log_{b}(x) − log_{b}(y)':'logarithm laws log rules division',
 'log_{b}(x^{n}) = n log_{b}(x)':'logarithm laws log rules exponent',
 'log_{b}(x) = y ⇔ b^{y} = x':'logarithm inverse exponential',
 'a^{m}a^{n} = a^{m+n}':'exponent laws index laws powers',
 '(a^{m})^{n} = a^{mn}':'exponent laws index laws power of a power',
 'a^{−n} = 1/a^{n}':'negative exponent reciprocal',
 'a^{1/n} = ^{n}√a':'fractional exponent radical root',
 'y = y_{0} e^{kt}':'exponential growth decay model',
 '(x − h)^{2} + (y − k)^{2} = r^{2}':'equation of a circle conic',
 'x^{2}/a^{2} + y^{2}/b^{2} = 1':'conic ellipse equation major minor axis',
 'x^{2}/a^{2} − y^{2}/b^{2} = 1':'conic hyperbola equation',
 'y^{2} = 4 p x':'conic focus directrix',
 'S_{n} = a_{1}(1 − r^{n})/(1 − r)':'sum of a geometric progression gp',
 'S = a_{1}/(1 − r)':'sum to infinity convergent geometric series',
 'a_{n} = a_{1} + (n − 1) d':'arithmetic progression ap nth term',
 'S_{n} = n(a_{1} + a_{n})/2':'sum of an arithmetic progression ap',
 # differential equations
 'dy/dt + p y = q':'first order linear ode',
 'μ = e^{∫ p dt}':'first order linear ode',
 '∫ dy/g(y) = ∫ f(t) dt':'separation of variables separable ode',
 'a y′′ + b y′ + c y = 0':'second order linear ode constant coefficient homogeneous',
 'a r^{2} + b r + c = 0':'auxiliary equation ode',
 'x′ = A x':'system of odes eigenvalues phase plane',
 '∂u/∂t = α ∂^{2}u/∂x^{2}':'pde diffusion equation parabolic',
 '∂^{2}u/∂t^{2} = c^{2} ∂^{2}u/∂x^{2}':'pde hyperbolic vibrating string',
 '∂^{2}u/∂x^{2} + ∂^{2}u/∂y^{2} = 0':'pde elliptic harmonic potential',
 # numerical methods
 'x_{n+1} = x_{n} − f(x_{n})/f′(x_{n})':'newton raphson root finding',
 '∫ f dx ≈ (h/2)(f_{0} + 2f_{1} + 2f_{2} + … + f_{n})':'numerical integration quadrature trapezium trapezoidal',
 '∫ f dx ≈ (h/3)(f_{0} + 4f_{1} + 2f_{2} + 4f_{3} + … + f_{n})':'numerical integration quadrature simpsons rule',
 'y_{n+1} = y_{n} + h f(t_{n}, y_{n})':'eulers method forward euler numerical ode',
 'y_{n+1} = y_{n} + (k_{1} + 2k_{2} + 2k_{3} + k_{4})/6':'runge kutta numerical ode',
 'x = (A^{T} A)^{−1} A^{T} b':'linear regression curve fit',
 '|x − x_{a}|/|x|':'percent error percentage error',
 # heat transfer
 'Q̇ = −k A dT/dx':'heat conduction thermal conductivity',
 'Q̇ = h A (T − T_{∞})':'convection heat transfer coefficient',
 'Q̇ = ε σ A (T_{1}^{4} − T_{2}^{4})':'thermal radiation stefan boltzmann emissivity',
 '1/(U A) = Σ R':'overall heat transfer coefficient composite wall',
 'ΔT_{m} = (ΔT_{1} − ΔT_{2})/ln(ΔT_{1}/ΔT_{2})':'lmtd heat exchanger',
 'Q̇ = U A ΔT_{m}':'lmtd heat exchanger sizing',
 'Nu = 0.023 Re^{0.8} Pr^{n}':'forced convection pipe correlation',
 'T − T_{∞} = (T_{0} − T_{∞}) e^{−t/τ}':'transient cooling newtons law of cooling time',
 'Bi = h L/k':'biot lumped',
 # computer science
 'T(n) = a T(n/b) + f(n)':'big o recurrence relation divide and conquer complexity',
 'Σ_{i=1}^{n} i = n(n + 1)/2':'big o complexity gauss sum',
 'H = −Σ p_{i} log_{2}(p_{i})':'information theory entropy bits',
 'S = 1/((1 − p) + p/s)':'amdahl parallel speedup',
 'L = λ W':'queueing theory littles law',
 't = L/R':'network delay',
 'BDP = R RTT':'bdp tcp window',
 'α = n/m':'hash table',
 # machine learning
 'w ← w − η ∇L':'sgd stochastic gradient descent learning rate optimizer',
 'σ(z) = 1/(1 + e^{−z})':'logistic function activation',
 'ReLU(z) = max(0, z)':'relu activation function',
 'p_{i} = e^{z_{i}}/(Σ_{j} e^{z_{j}})':'softmax activation multiclass',
 'MSE = (1/n) Σ (y_{i} − ŷ_{i})^{2}':'mse loss l2 loss',
 'RMSE = √(MSE)':'rmse',
 'MAE = (1/n) Σ |y_{i} − ŷ_{i}|':'mae l1 loss',
 'R^{2} = 1 − SS_{res}/SS_{tot}':'r squared r2 goodness of fit',
 'L = −Σ y_{i} ln(ŷ_{i})':'cross entropy log loss nll',
 'L = −(y ln(ŷ) + (1 − y) ln(1 − ŷ))':'bce log loss',
 'D = Σ p_{i} ln(p_{i}/q_{i})':'kl kullback leibler relative entropy',
 'A = softmax(Q K^{T}/√(d_{k})) V':'attention transformer self attention qkv',
 'P = TP/(TP + FP)':'precision ppv positive predictive value',
 'R = TP/(TP + FN)':'recall sensitivity tpr hit rate',
 'TNR = TN/(TN + FP)':'specificity',
 'FPR = FP/(FP + TN)':'roc auc fall-out',
 'F_{1} = 2PR/(P + R)':'f1 f-score f measure',
 'A = (TP + TN)/(TP + TN + FP + FN)':'accuracy confusion matrix',
 'L + λ ‖w‖^{2}':'ridge weight decay l2 penalty',
 'L + λ ‖w‖_{1}':'lasso l1 penalty sparsity',
 # astronomy
 'T^{2} = 4π^{2} a^{3}/(G M)':'orbital period kepler third law',
 'v = √(G M/r)':'orbital velocity orbital speed satellite',
 'v^{2} = G M (2/r − 1/a)':'vis viva orbital speed ellipse',
 'd = 1/p':'parsec parallax distance',
 'v = H_{0} d':'hubble constant expansion of the universe',
 'm − M = 5 log_{10}(d) − 5':'absolute magnitude apparent magnitude',
 'L = 4π R^{2} σ T^{4}':'stellar luminosity',
 'θ = 1.22 λ/D':'angular resolution resolving power',
 # finance
 'A = P(1 + r/n)^{n t}':'compound interest apr',
 'A = P e^{r t}':'continuously compounded interest',
 'M = P r/(1 − (1 + r)^{−n})':'mortgage loan payment emi amortization',
 'PV = FV/(1 + r)^{n}':'discounting discount factor time value of money',
 'NPV = Σ C_{t}/(1 + r)^{t}':'npv irr internal rate of return dcf discounted cash flow',
 'PV = C/(r − g)':'gordon growth dividend discount model ddm',
 'EAR = (1 + r/n)^{n} − 1':'apy apr effective interest rate',
 'E[R] = r_{f} + β (E[R_{m}] − r_{f})':'capm beta expected return stock risk premium security market line cost of equity',
 'B = P (1 + r)^{k} − M ((1 + r)^{k} − 1)/r':'remaining balance outstanding principal payoff amount',
 'D = (C − S)/n':'salvage value book value depreciation',
 'PM = (R − C)/R':'net margin gross margin markup',
 # geometry
 'a^{2} + b^{2} = c^{2}':'pythagoras pythagorus pythagorian hypotenuse right triangle',
 'ROI = gain/cost':'roi return on investment',
 # chemistry
 'n = m/M':'moles molar mass stoichiometry',
 'c = n/V':'molarity concentration mol/l',
 'P V = n R T':'pv = nrt ideal gas',
 'ΔG = ΔH − T ΔS':'gibbs free energy spontaneity',
 'pH = −log_{10}[H^{+}]':'ph acidity hydrogen ion',
 'K = ([C]^{c} [D]^{d})/([A]^{a} [B]^{b})':'equilibrium constant kc law of mass action',
 'k = A e^{−E_{a}/(RT)}':'activation energy arrhenius',
 'E = E° − (RT/(nF)) ln(Q)':'nernst cell potential',
 'A = ε l c':'absorbance beer lambert spectrophotometry',
 'c_{1} V_{1} = c_{2} V_{2}':'dilution m1v1 = m2v2',
 'q = m c ΔT':'specific heat capacity calorimetry heat',
 'ΔT_{f} = i K_{f} b':'freezing point depression colligative',
 'ΔT_{b} = i K_{b} b':'boiling point elevation colligative',
 # biology
 'dN/dt = r N (1 − N/K)':'logistic growth carrying capacity',
 'p^{2} + 2pq + q^{2} = 1':'hardy weinberg equilibrium',
 'v = V_{max} [S]/(K_{m} + [S])':'michaelis menten enzyme kinetics km vmax',
 'χ^{2} = Σ (O − E)^{2}/E':'chi squared goodness of fit',
 'H = −Σ p_{i} ln(p_{i})':'shannon wiener diversity',
 'N = M C/R':'capture recapture population estimate lincoln',
 'E = (RT/(zF)) ln([X]_{out}/[X]_{in})':'equilibrium potential nernst membrane',
 'CO = HR × SV':'cardiac output heart rate stroke volume',
 # continuum mechanics
 'σ = F/A':'engineering stress normal stress',
 'ε = ΔL/L':'engineering strain',
 'σ = E ε':'hookes law youngs modulus elasticity',
 'σ_{v} = √(((σ_{1} − σ_{2})^{2} + (σ_{2} − σ_{3})^{2} + (σ_{3} − σ_{1})^{2})/2)':'von mises equivalent stress yield criterion distortion energy',
 'σ_{1,2} = (σ_{x} + σ_{y})/2 ± √(((σ_{x} − σ_{y})/2)^{2} + τ_{xy}^{2})':'mohrs circle principal stresses',
 'C = (σ_{x} + σ_{y})/2':'mohrs circle',
 'δ = PL/(AE)':'axial deformation elongation deflection',
 'τ = Tρ/J':'torsion shear stress shaft',
 'φ = TL/(GJ)':'angle of twist torsion',
 'σ = −My/I':'bending stress flexure formula beam',
 'τ = VQ/(It)':'transverse shear stress beam',
 'P_{cr} = π^{2}EI/(KL)^{2}':'euler buckling column critical load',
 'I = bh^{3}/12':'second moment of area rectangle',
 'σ_{h} = pr/t':'hoop stress thin walled pressure vessel',
 'p + ρv^{2}/2 + ρgz = constant':'bernoulli equation',
 'Re = ρvD/μ':'reynolds number laminar turbulent',
 'h_{f} = f (L/D) v^{2}/(2g)':'darcy weisbach head loss pipe friction',
 'F_{D} = (1/2) C_{D} ρ v^{2} A':'drag equation air resistance',
 # statics and dynamics
 'Σ F = 0':'statics equilibrium free body diagram fbd',
 'f ≤ μ N':'friction coefficient static kinetic',
 'I = Ī + m d^{2}':'parallel axis theorem steiner',
 'ω_{n} = √(k/m)':'natural frequency spring mass',
 'ζ = c/c_{c}':'damping ratio',
 'W = Δ KE':'work energy principle',
 'e = (v′_{2} − v′_{1})/(v_{1} − v_{2})':'coefficient of restitution collision',
 'T_{2} = T_{1} e^{μβ}':'capstan belt friction rope',
 'X = (F_{0}/k)/√((1 − r^{2})^{2} + (2ζr)^{2})':'resonance frequency response dynamic amplification',
 'a⃗ = (r̈ − r θ̇^{2}) r̂ + (r θ̈ + 2 ṙ θ̇) θ̂':'polar coordinates radial transverse coriolis',
 'a⃗ = v̇ t̂ + (v^{2}/ρ) n̂':'normal tangential path coordinates centripetal',
}.items():
    TAGS[k]=(TAGS.get(k,'')+' '+v).strip()
# round 2: what students typed that found nothing (sheet search, every query in its top 3)
for k,v in {
 'p(x) = (x − r) q(x) + p(r)':'synthetic division polynomial long division',
 'x = r cos(θ)':'polar to rectangular conversion coordinates',
 'y = r sin(θ)':'polar to rectangular conversion coordinates',
 'r^{2} = x^{2} + y^{2}':'rectangular to polar conversion coordinates',
 'm = (y_{2} − y_{1})/(x_{2} − x_{1})':'line through two points',
 '(f(x + h) − f(x))/h':'secant slope average rate of change derivative definition',
 'Σ_{i=1}^{n} i = n(n + 1)/2':'sum 1 to n first n integers triangular numbers',
 'f′(x) ≈ (f(x + h) − f(x))/h':'numerical derivative',
 'f′(x) ≈ (f(x) − f(x − h))/h':'numerical derivative',
 'f′(x) ≈ (f(x + h) − f(x − h))/(2h)':'numerical derivative',
 'x_{i}^{(k+1)} = (b_{i} − Σ_{j≠i} a_{ij} x_{j}^{(k)})/a_{ii}':'gauss-seidel iterative solver linear system',
 'x_{k+1} = A x_{k}/‖A x_{k}‖':'dominant eigenvalue spectral radius eigenvector',
 'k_{eq} = k_{1} + k_{2}':'equivalent stiffness spring constant',
 '1/k_{eq} = 1/k_{1} + 1/k_{2}':'equivalent stiffness spring constant',
 'Nu = 0.023 Re^{0.8} Pr^{n}':'turbulent flow',
 'ε = α':'emissivity absorptivity gray body radiation',
 't = L/R':'latency',
 't = d/s':'latency',
 'd = d_{proc} + d_{queue} + d_{trans} + d_{prop}':'latency',
 'L = λ W':'throughput queue length',
 'w = (X^{T} X)^{−1} X^{T} y':'ols',
 'd(x, y) = √(Σ (x_{i} − y_{i})^{2})':'knn nearest neighbors kmeans clustering distance metric',
 'v ← β v + ∇L,  w ← w − η v':'sgd momentum optimizer',
 'σ′(z) = σ(z)(1 − σ(z))':'backpropagation backprop gradient',
 'σ(z) = 1/(1 + e^{−z})':'neural network',
 'tanh(z) = (e^{z} − e^{−z})/(e^{z} + e^{−z})':'neural network activation function',
 'ReLU(z) = max(0, z)':'neural network',
 'L + λ ‖w‖^{2}':'overfitting regularisation',
 'L + λ ‖w‖_{1}':'overfitting regularisation',
 'FPR = FP/(FP + TN)':'type I error false alarm',
 'R = TP/(TP + FN)':'type II error',
 'm_{1} − m_{2} = −2.5 log_{10}(F_{1}/F_{2})':'star brightness flux ratio',
 'λ_{max} = b/T':'star stellar temperature colour peak wavelength',
 'd = 1/p':'light years',
 'z = Δλ/λ':'doppler shift',
 'v = c z':'doppler shift',
 'M = P r/(1 − (1 + r)^{−n})':'car loan monthly payment amortization',
 'FV = PMT ((1 + r)^{n} − 1)/r':'savings plan retirement sinking fund regular deposits',
 'PV = PMT (1 − (1 + r)^{−n})/r':'retirement payout pension',
 'PV = C/(r − g)':'stock valuation',
 'PM = (R − C)/R':'net profit',
 'P V = n R T':'pv=nrt',
 'Q < K':'le chatelier direction of shift Q > K backward',
 '[A]_{0}/(2k)':'zero order reaction kinetics integrated rate law',
 'ln(2)/k':'first order reaction kinetics integrated rate law',
 '1/(k[A]_{0})':'second order reaction kinetics integrated rate law',
 'E = P_{n+1}/P_{n}':'10% rule ten percent energy pyramid food chain',
 '(a × b)_{i} = ε_{ijk} a_{j} b_{k}':'levi-civita alternating tensor cross product',
 'ε_{ijk} ε_{imn} = δ_{jm} δ_{kn} − δ_{jn} δ_{km}':'levi-civita epsilon delta identity',
 'dV/dx = −w':'shear and moment diagrams',
 'dM/dx = V':'shear and moment diagrams',
 'I = bh^{3}/12':'area moment of inertia',
 # what a shortened note no longer says
 'r_{p} = a(1 − e),  r_{a} = a(1 + e)':'perihelion aphelion closest farthest',
 'MAP ≈ DBP + (SBP − DBP)/3':'mean arterial pressure blood pressure',
 'RF = recombinants/total':'recombination frequency map unit centimorgan',
 'N = M C/R':'marked caught recaptured',
 'K_{a} = [H^{+}][A^{−}]/[HA]':'acid dissociation constant',
 'pH = pK_{a} + log_{10}([A^{−}]/[HA])':'buffer',
 "ln(K_{2}/K_{1}) = −(ΔH°/R)(1/T_{2} − 1/T_{1})":'equilibrium constant temperature',
 'T_{2} = T_{1} e^{μβ}':'wrap angle',
 'WACC = (E/V) r_{E} + (D/V) r_{D} (1 − T)':'weighted average',
 'P = Σ_{t=1}^{n} C/(1 + y)^{t} + F/(1 + y)^{n}':'coupon face value yield',
 'Q = F/(p − v)':'fixed cost variable cost price',
 'B = P (1 + r)^{k} − M ((1 + r)^{k} − 1)/r':'loan balance remaining',
 'κ(A) = ‖A‖ ‖A^{−1}‖':'condition number',
 'm_{1} m_{2} = −1':'perpendicular slopes parallel lines',
 'θ = 1.22 λ/D':'radians',
 'L/L_{sun} = (M/M_{sun})^{3.5}':'relation',
 'M = f_{o}/f_{e}':'objective eyepiece focal length',
}.items():
    TAGS[k]=(TAGS.get(k,'')+' '+v).strip()
# round 3: symbols typed as letters, misspellings and course phrases that still missed
for k,v in {
 # statics and dynamics
 'ω_{n} = √(k/m)':'omega n wn',
 'ω_{d} = ω_{n} √(1 − ζ^{2})':'omega d wd',
 '^{A}α⃗^{B} = ^{A}d(^{A}ω⃗^{B})/dt = ^{B}d(^{A}ω⃗^{B})/dt':'alpha angular acceleration',
 'Σ M = I α':'alpha angular acceleration torque',
 'f ≤ μ N':'mu',
 'δ = ln(x_{1}/x_{2})':'delta',
 'ζ = c/c_{c}':'overdamped underdamped critically damped zeta',
 'M_{1} = I_{1} ω̇_{1} − (I_{2} − I_{3}) ω_{2} ω_{3}':"euler's equations rigid body gyroscope precession torque-free spinning top",
 '^{N}a⃗^{Q} = ^{N}a⃗^{B_{o}} + ^{N}α⃗^{B} × ^{B_{o}}r⃗^{Q} + ^{N}ω⃗^{B} × (^{N}ω⃗^{B} × ^{B_{o}}r⃗^{Q})':'relative acceleration two points on a rigid body',
 '^{N}v⃗^{Q} = ^{N}v⃗^{B_{o}} + ^{N}ω⃗^{B} × ^{B_{o}}r⃗^{Q}':'relative velocity two points on a rigid body',
 '^{A}dv⃗/dt = ^{B}dv⃗/dt + ^{A}ω⃗^{B} × v⃗':'transport theorem rotating frame derivative',
 'Momentum is conserved along û if no external force (or moment) acts there':'conservation of angular momentum linear momentum',
 '^{O}r⃗^{S_{cm}} = (1/m^{S}) Σ_{i=1}^{n} m^{Q_{i}} ^{O}r⃗^{Q_{i}}':'centre of mass center of mass center of gravity cm',
 'x̄ = (Σ A_{i} x_{i})/(Σ A_{i})':'center of gravity composite shapes',
 'M⃗^{F⃗∕O} = ^{O}r⃗^{Q} × F⃗':'torque r × f cross product moment of a force',
 '[I] = [I_{xx}, I_{xy}, I_{xz}; I_{xy}, I_{yy}, I_{yz}; I_{xz}, I_{yz}, I_{zz}]':'inertia tensor inertia matrix',
 '−∂U/∂q_{r} = d(∂K/∂q̇_{r})/dt − ∂K/∂q_{r}':"lagrangian lagrange's equations virtual work generalized coordinates generalized forces",
 'F⃗^{S} · û = m^{S} ^{N}a⃗^{S_{cm}} · û':"kane's method virtual work",
 # heat transfer
 'Pr = μ c_{p}/k':'pr',
 'α = k/(ρ c_{p})':'alpha',
 'η_{f} = tanh(m L)/(m L)':'eta',
 'E_{b} = σ T^{4}':'sigma t^4 stefan boltzmann',
 'R = L/(k A)':'r-value r value insulation',
 'ΔT_{m} = (ΔT_{1} − ΔT_{2})/ln(ΔT_{1}/ΔT_{2})':'heat exchanger shell and tube parallel flow counterflow',
 'Q̇ = U A ΔT_{m}':'heat exchanger shell and tube',
 # numerical methods
 'κ(A) = ‖A‖ ‖A^{−1}‖':'kappa ill-conditioned',
 # computer science
 'T(n) = a T(n/b) + f(n)':'big o big oh asymptotic complexity runtime',
 'Σ deg(v) = 2E':'degree sum',
 'log_{2}(n!) ≈ n log_{2}(n)':'log n factorial lg(n!)',
 't = L/R':'throughput bit rate',
 'BDP = R RTT':'throughput',
 # machine learning
 'L = −(y ln(ŷ) + (1 − y) ln(1 − ŷ))':'logloss loss function',
 'L = −Σ y_{i} ln(ŷ_{i})':'loss function cost function',
 'MSE = (1/n) Σ (y_{i} − ŷ_{i})^{2}':'loss function cost function',
 'FPR = FP/(FP + TN)':'type 1 error type i false alarm fall-out',
 # astronomy
 'T^{2} = 4π^{2} a^{3}/(G M)':'semi major axis orbital period',
 'r_{s} = 2 G M/c^{2}':'event horizon',
 'd = 1/p':'distance to a star light years',
 'θ = D/d':'angular diameter angular size',
 'F = L/(4π d^{2})':'apparent brightness',
 # continuum mechanics
 'f = 64/Re':'moody diagram chart',
 'h_{f} = f (L/D) v^{2}/(2g)':'moody diagram chart',
 'dB/dt = d/dt ∫_{CV} ρb dV + ∫_{CS} ρb (v · n) dA':'control volume',
}.items():
    TAGS[k]=(TAGS.get(k,'')+' '+v).strip()
# critic round 4, an exam walkthrough (physics, circuits, heat transfer, continuum mechanics, chemistry, biology,
# astronomy): what a student types in a timed exam for the rows added then and the rows a search missed
for k,v in {
 'x = v_{0} cos(θ) t':'projectile horizontal velocity horizontal motion position',
 'x(t) = x(∞) + (x(0) − x(∞)) e^{−t/τ}':'rc rl charging discharging capacitor voltage',
 'y = y_{0} + v_{0} sin(θ) t − (1/2) g t^{2}':'projectile motion cliff launched from height vertical motion trajectory',
 'tan(θ) = v^{2}/(r g)':'banked curve banking angle',
 'T^{2} = 4π^{2} a^{3}/(G M)':'orbital period satellite period of orbit',
 'q = Q (1 − e^{−t/(RC)})':'rc circuit capacitor charging time constant',
 's^{2} + (R/L) s + 1/(LC) = 0':'characteristic equation rlc second order circuit',
 'v(t) = V_{f} + A_{1} e^{s_{1}t} + A_{2} e^{s_{2}t}':'rlc step response overdamped natural response two real roots',
 'v(t) = V_{f} + (A_{1} + A_{2} t) e^{−αt}':'rlc step response critically damped critical damping',
 'v(t) = V_{f} + e^{−αt} (B_{1} cos(ω_{d}t) + B_{2} sin(ω_{d}t))':'rlc step response underdamped natural response ringing complex roots',
 'L_{c} = L + A_{c}/P':'corrected fin length convective tip convection tip',
 'Q̇ = √(h P k A_{c}) (T_{b} − T_{∞})':'infinite fin long fin heat loss',
 'ε_{fin} = Q̇_{fin}/(h A_{c} (T_{b} − T_{∞}))':'fin effectiveness',
 'T(x) = T_{s} + (q̇ L^{2}/(2k))(1 − x^{2}/L^{2})':'heat generation internal generation maximum temperature wall',
 'T(r) = T_{s} + (q̇ r_{o}^{2}/(4k))(1 − r^{2}/r_{o}^{2})':'heat generation internal generation maximum temperature wire',
 'Nu = C Ra^{n}':'natural convection free convection rayleigh',
 '(T_{s} − T_{out})/(T_{s} − T_{in}) = e^{−h P L/(ṁ c_{p})}':'outlet temperature mean temperature pipe exit temperature',
 'ΔT_{1} = T_{h,in} − T_{c,out},  ΔT_{2} = T_{h,out} − T_{c,in}':'counterflow heat exchanger counter flow',
 'ΔT_{1} = T_{h,in} − T_{c,in},  ΔT_{2} = T_{h,out} − T_{c,out}':'parallel flow heat exchanger',
 'ε = (1 − e^{−NTU(1 + c)})/(1 + c)':'parallel flow heat exchanger',
 's = −α ± √(α^{2} − ω_{0}^{2})':'characteristic roots rlc roots',
 'NTU = U A/C_{min}':'effectiveness ntu method',
 'Q̇_{12} = σ (T_{1}^{4} − T_{2}^{4})/((1 − ε_{1})/(A_{1} ε_{1}) + 1/(A_{1} F_{12}) + (1 − ε_{2})/(A_{2} ε_{2}))':'radiation between two surfaces gray body radiation exchange network',
 'ȳ = (Σ A_{i} y_{i})/(Σ A_{i})':'neutral axis centroid composite section t-beam',
 'σ = P/A ± Mc/I':'combined loading axial and bending eccentric load',
 'R = √(((σ_{x} − σ_{y})/2)^{2} + τ_{xy}^{2})':'mohrs circle mohr circle radius',
 'n_{B} = (b/a) n_{A}':'stoichiometry limiting reagent limiting reactant mole ratio theoretical yield',
 'K = ([C]^{c} [D]^{d})/([A]^{a} [B]^{b})':'kc ice table equilibrium constant expression',
 'c_{a} V_{a} = c_{b} V_{b}':'titration equivalence point acid base',
 't_{d} ≈ 70/r':'rule of 70 rule of seventy doubling time',
 '1/S = 1/P_{1} − 1/P_{2}':'synodic period',
 'E = −G M m/(2a)':'orbital energy total energy orbit',
 'M_{bol} = 4.74 − 2.5 log_{10}(L/L_{sun})':'absolute magnitude luminosity bolometric',
}.items():
    TAGS[k]=(TAGS.get(k,'')+' '+v).strip()
# critic round 5: the ε-NTU rows by their symbol and method name, a radiation shield
for k,v in {
 'NTU = U A/C_{min}':'ε-NTU epsilon-NTU',
 'ε = Q̇/Q̇_{max}':'ε-NTU epsilon-NTU effectiveness-NTU method',
 'ε = (1 − e^{−NTU(1 − c)})/(1 − c e^{−NTU(1 − c)})':'ε-NTU epsilon-NTU effectiveness-NTU method',
 'ε = (1 − e^{−NTU(1 + c)})/(1 + c)':'ε-NTU epsilon-NTU effectiveness-NTU method',
 'ε = 1 − e^{−NTU}':'ε-NTU epsilon-NTU effectiveness-NTU method',
 'q″ = σ (T_{1}^{4} − T_{2}^{4})/(1/ε_{1} + 1/ε_{2} − 1)':'radiation shield',
}.items():
    TAGS[k]=(TAGS.get(k,'')+' '+v).strip()
native=[precalc,diffeq,numerical,dynamics,heat,compsci,mlsheet,astronomy,finance,chemistry,biology,continuum,fourier,euler,complexn,ct,dt,laplace,ztr,circle,geom,stats,linalg,physics,circuits,taylor,vector]
def row(cmd,what): return {'lead':[cmd],'show':what,'copy':cmd,'open':cmd}
index=SH('formulas','Formulas','all the formula sheets',
 S('Click one to open it, or type its name',
  G('Signals and transforms',row('fourier','Fourier series'),row('euler',"Euler's formulas"),row('complex numbers','special values, polar form'),
    row('ct transform table','CT Fourier transforms'),row('dt transform table','DT Fourier transforms'),row('laplace','Laplace transforms'),row('z transform','Z transforms')),
  G('Math',row('identities','trig, logs, algebra, sums'),row('calculus','derivatives and integrals'),row('unit circle','exact trig values'),row('taylor series','Maclaurin series'),row('precalculus','lines, quadratics, logs, functions, conics, sequences'),row('differential equations','first and second order ODEs, forcing, systems, PDEs'),row('numerical methods','Newton, bisection, interpolation, Simpson, Euler, RK4'),
    row('linear algebra','matrices, vectors, eigenvalues'),row('vector calculus','operators and theorems'),row('geometry','area, volume, triangles, circles, coordinates')),
  G('Science and engineering',row('physics','mechanics, E&M, quantum'),row('chemistry','moles, gases, thermochemistry, equilibrium, acids, kinetics'),row('biology','populations, genetics, enzymes, membranes, physiology'),row('continuum mechanics','stress, Mohr, beams, torsion, buckling, fluids'),row('electrical engineering','AC, transistors, logic'),row('dynamics','statics, friction, inertia, momentum, energy, vibrations'),row('heat transfer','conduction, fins, convection, exchangers, radiation'),row('astronomy','orbits, magnitudes, stars, redshift, telescopes'),row('statistics','probability, distributions, confidence intervals, hypothesis tests'),row('computer science','big O, recurrences, hashing, graphs, bits, caches, networks'),row('machine learning','regression, activations, neural networks, losses, optimizers, clustering, metrics'),row('finance','interest, present value, annuities, loans, NPV, IRR, CAPM, bonds'),
    row('constants','physical constants'),row('conversions','units and metric prefixes'),row('periodic','periodic table')),
  wide=True,leads=[140]))
native.append(index)
def circle_figure():
    "the unit circle: one Typst page of lines, and each label its own document, so a click knows exactly where it is"
    half=340; R=232; arrow=330
    def doc(size,inner): return '#set page(width: auto, height: auto, margin: 1pt, fill: none)\n#set text(size: %dpt, fill: black)\n%s\n'%(size,inner)
    out=['#set page(width: %dpt, height: %dpt, margin: 0pt, fill: none)'%(half*2,half*2),
         '#set text(size: 15pt, fill: black)',
         '#let at(x, y, body) = place(center + horizon, dx: x * 1pt, dy: y * 1pt, body)',
         '#let seg(x1, y1, x2, y2, w) = place(top + left, line(start: ((%d + x1) * 1pt, (%d + y1) * 1pt), end: ((%d + x2) * 1pt, (%d + y2) * 1pt), stroke: w * 1pt + black))'%(half,half,half,half),
         '#let tip(x, y, x2, y2, x3, y3) = place(top + left, polygon(fill: black, stroke: none, ((%d + x) * 1pt, (%d + y) * 1pt), ((%d + x2) * 1pt, (%d + y2) * 1pt), ((%d + x3) * 1pt, (%d + y3) * 1pt)))'%(half,half,half,half,half,half),
         '#at(0, 0, circle(radius: %dpt, stroke: 1pt + black))'%R]
    spots=[]
    for dx,dy in [(1,0),(-1,0),(0,1),(0,-1)]:
        out.append('#seg(%g, %g, %g, %g, 1)'%(dx*R,dy*R,dx*arrow,dy*arrow))
        tx,ty=dx*(arrow+8),dy*(arrow+8); bx,by=dx*(arrow-4),dy*(arrow-4)
        out.append('#tip(%g, %g, %g, %g, %g, %g)'%(tx,ty,bx+dy*5,by+dx*5,bx-dy*5,by-dx*5))
    out.append('#at(%d, -14, $x$)'%(arrow-2)); out.append('#at(14, %d, $y$)'%(-(arrow-2)))
    for d,r,sn,c,_ in circle_rows:
        if d==360: continue
        a=math.radians(d); ux,uy=math.cos(a),-math.sin(a)
        axis=d%90==0
        for r1,r2 in [(0,.51),(.665,.71),(.93,1)]:
            out.append('#seg(%.2f, %.2f, %.2f, %.2f, %s)'%(ux*r1*R,uy*r1*R,ux*r2*R,uy*r2*R,'1' if axis else '0.5'))
        out.append('#place(top + left, dx: %.2fpt, dy: %.2fpt, circle(radius: 2.8pt, fill: black, stroke: none))'%(half+ux*R-2.8,half+uy*R-2.8))
        spots.append({'x':round(half+ux*.59*R,1),'y':round(half+uy*.59*R,1),'doc':doc(14 if axis else 11,'#"%d°"'%d),'copy':'%d°'%d})
        spots.append({'x':round(half+ux*.82*R,1),'y':round(half+uy*.82*R,1),'doc':doc(14,'$ %s $'%body(r)),'copy':plain(r)})
        if d==0: x,y=R+36,24
        elif d==90: x,y=40,-R-20
        elif d==180: x,y=-R-36,-24
        elif d==270: x,y=-40,R+20
        else: x,y=ux*(R+68),uy*(R+40)
        # the point, piece by piece: each coordinate copies alone; the brackets and comma are only drawn
        wide={'0':8,'1':8,'−1':17,'1/2':12,'−1/2':21,'√2/2':22,'√3/2':22,'−√2/2':31,'−√3/2':31}
        paren,comma=6,8
        tall=d%90!=0
        left=x-(2*paren+comma+wide[c]+wide[sn])/2
        def drawn(cx,inner,size=13,copy=''):
            spots.append({'x':round(half+cx,1),'y':round(half+y,1),'doc':doc(size,inner),'copy':copy})
        bracket=30 if tall else 14
        drawn(left+paren/2,'#"("',bracket)
        at=left+paren
        drawn(at+wide[c]/2,'$ %s $'%body(c),13,plain(c)); at+=wide[c]
        drawn(at+comma/2,'#","'); at+=comma
        drawn(at+wide[sn]/2,'$ %s $'%body(sn),13,plain(sn)); at+=wide[sn]
        drawn(at+paren/2,'#")"',bracket)
    return '\n'.join(out)+'\n',spots
UPRIGHT={'Cs','Si','Ry','PV','FV','NPV','PMT','ROI','MSE','TP','FP','FN','TN','Nu','Bi','Pr','CPI','RTT','hit','real','miss','and','mW','BW','SR','GBW','SNR','dBm','LSB','PF','OS','to','GS','DS','BE','ox','ov','ph','FS','ref','clk','cq','su','dc','pk','cm','dB','pH','pOH','pK','HA','OH','vap','tot','sp','eV','mV','kJ','mol','RF','CO','HR','SV','MAP','TPR','BMI','app','cat','out','max','cr','lat','CV','CS','along','for','Re','Im','Var','KE','PE','CT','DT','out','in','th','oc','sc','rms','dB','Ptr','FS','abs','atm','SG','Ma'}
OPS={'sin','cos','tan','sinh','cosh','ln','log','exp','lim','det','tr','cot','sec','csc','arcsin','arccos','arctan'}
OVERRIDE={
 # the dynamics sheet's matrices
 'a⃗ × b⃗ = det[x̂ ŷ ẑ; a_{x} a_{y} a_{z}; b_{x} b_{y} b_{z}]':'arrow(a) times arrow(b) = det mat(hat(x), hat(y), hat(z); a_x, a_y, a_z; b_x, b_y, b_z)',
 'R_{z} = [cos θ, −sin θ, 0; sin θ, cos θ, 0; 0, 0, 1]':'R_z = mat(cos θ, -sin θ, 0; sin θ, cos θ, 0; 0, 0, 1)',
 'R_{x} = [1, 0, 0; 0, cos θ, −sin θ; 0, sin θ, cos θ]':'R_x = mat(1, 0, 0; 0, cos θ, -sin θ; 0, sin θ, cos θ)',
 'R_{y} = [cos θ, 0, sin θ; 0, 1, 0; −sin θ, 0, cos θ]':'R_y = mat(cos θ, 0, sin θ; 0, 1, 0; -sin θ, 0, cos θ)',
 '[I] = [I_{xx}, I_{xy}, I_{xz}; I_{xy}, I_{yy}, I_{yz}; I_{xz}, I_{yz}, I_{zz}]':'[I] = mat(I_(x\\x), I_(x\\y), I_(x\\z); I_(x\\y), I_(y\\y), I_(y\\z); I_(x\\z), I_(y\\z), I_(z\\z))',
 'M(^{12}C)':'M(""^12 "C")',
 'a^{1/n} = ^{n}√a':'a^(1/n) = root(n, a)',
 '|x − x_{a}|/|x|':'(|x - x_a|)/(|x|)',
 '20 dB/decade':'20 "dB/decade"',
 '6CO_{2} + 6H_{2}O → C_{6}H_{12}O_{6} + 6O_{2}':'6"CO"_2 + 6"H"_2"O" arrow.r "C"_6"H"_12"O"_6 + 6"O"_2',
 'C_{6}H_{12}O_{6} + 6O_{2} → 6CO_{2} + 6H_{2}O':'"C"_6"H"_12"O"_6 + 6"O"_2 arrow.r 6"CO"_2 + 6"H"_2"O"',
 '1 for |ω| < W, 0 else':'cases(1\\, & |ω| < W, 0\\, & "else")',
 '1 for |t| < T_{1}, 0 else':'cases(1\\, & |t| < T_1, 0\\, & "else")',
 '1 for |n| ≤ N_{1}, 0 else':'cases(1\\, & |n| <= N_1, 0\\, & "else")',
 'A^{−1} = (1/det A) [d −b; −c a]':'A^(-1) = 1/(det A) mat(d, -b; -c, a)',
 'det A = ad − bc':'det A = a\\d - b\\c',
 'tan^{−1} x = x − x^{3}/3 + x^{5}/5 − …':'tan^(-1) x = x - x^3/3 + x^5/5 - dots',
}
PAIRS={'(':')','[':']','{':'}'}
def span_left(s,i):
    "the start of the operand ending just before i"
    depth=0; j=i-1
    while j>=0:
        c=s[j]
        if c in ')]}': depth+=1
        elif c in '([{':
            if depth==0: return j+1
            depth-=1
        elif c in ' ,=' and depth==0: return j+1
        j-=1
    return 0
def span_right(s,i):
    "the end of the operand starting at i"
    depth=0; j=i
    while j<len(s):
        c=s[j]
        if c in '([{': depth+=1
        elif c in ')]}':
            if depth==0: return j
            depth-=1
        elif c in ' ,=' and depth==0: return j
        j+=1
    return j
OP_ARG=r'(?:sin|cos|tan|cot|sec|csc|ln|log)(?:\^\{[^{}]*\}|_\{[^{}]*\})?'
def side(x):
    if x[:1]=='(':
        depth=0
        for n,c in enumerate(x):
            depth+=c=='('; depth-=c==')'
            if depth==0:
                if n==len(x)-1: return x
                break
    return '⟦'+x+'⟧'
def fractions(s):
    "every a/b with its two sides whole, so Typst's fraction takes all of each"
    out='' ; i=0
    s=s.replace(' / ','/')
    while True:
        k=s.find('/',i)
        if k<0: return s
        a=span_left(s,k); b=span_right(s,k+1)
        # `sin x` is one side, though it has a space in it
        before=re.search(OP_ARG+r' $',s[:a])
        if before: a=before.start()
        if re.fullmatch(OP_ARG,s[k+1:b]) and s[b:b+1]==' ':
            b=span_right(s,b+1)
        left=s[a:k]; right=s[k+1:b]
        # Typst drops one pair of brackets from each side, so a side that's already in a pair gets no more
        new=side(left)+'⁄'+side(right)
        s=s[:a]+new+s[b:]
        i=a+len(new)-len(right)-1-len(left)-3+len(left)+1 if False else a+1+len(left)
# a mark over a letter, as Typst draws it; I⃡ (a double arrow) is a dyadic, which Mitiguy draws with two arrows
ACCENTS={'\u20d7':'arrow','\u0302':'hat','\u0307':'dot','\u0308':'dot.double','\u0303':'tilde','\u20e1':'dyad'}
def accents(s):
    "r̂, θ̇, v⃗ and the rest as tokens walk knows; a ^{N} with nothing before it is written ahead of what follows"
    s=''.join(unicodedata.normalize('NFD',c) if any(m in unicodedata.normalize('NFD',c)[1:] for m in ACCENTS) else c for c in s)
    s=re.sub('(\\w)(['+''.join(ACCENTS)+'])',lambda m:'⟪%s:%s⟫'%(ACCENTS[m.group(2)],m.group(1)),s)
    # after an opening bar too (|^{P}r⃗^{Q}|), though not a closing one (|x|^{2})
    return re.sub(r'(^|[\s(=+−×·,\[]|(?:^|(?<=[\s=(]))\|)\^\{',r'\1⟪pre⟫^{',s)
def body(s):
    "typst maths for what a row shows"
    if s in OVERRIDE: return OVERRIDE[s]
    s=s.replace('ŷ','⟪yhat⟫').replace('ṁ','⟪mdot⟫').replace('Q̇','⟪Qdot⟫').replace('Ẇ','⟪Wdot⟫').replace('Ī','⟪Ibar⟫').replace('′','\'')
    s=s.replace('x̄','⟪xbar⟫').replace('ȳ','⟪ybar⟫').replace('f′','f\'').replace('0⁺','0^{+}').replace('<N>','⟪angle⟫').replace('…','⟪dots⟫')
    s=s.replace('F.S.','⟪FS⟫')
    s=accents(s)
    # (1/T) ahead of something is a plain coefficient: the fraction bar is bracket enough
    # only when a space follows: (π/j)[…] keeps its brackets, or the fraction would take the […] too
    s=re.sub(r'(?<![\w√)\]}])\(([^()\s]+/[^()\s]+)\)(?=\s|$)',r'\1',s)
    s=fractions(s)
    s=s.replace('⟦','(').replace('⟧',')').replace('⁄','/')
    # a little room before a differential: sin x dx, not sin xdx
    return re.sub(r'(?<=\s)d\\([^\W\d_])(?![\w\\])',r'thin d\\\1',walk(s))
UNITS=False
def unit_body(s):
    "a value and its unit: letters upright, a slash just a slash"
    global UNITS
    UNITS=True
    try: return walk(s.replace('…','⟪dots⟫'))
    finally: UNITS=False
def walk(s):
    class Out(list):
        # a letter straight after a letter or digit would read as one long name: keep them apart, with no gap
        def append(self,t):
            if self and t and self[-1] and self[-1][-1].isalpha() and t!='' and (t[0].isalpha() or t[0].isdigit()):
                list.append(self,'#h(0pt)')
            list.append(self,t)
    out=Out(); i=0
    while i<len(s):
        c=s[i]
        if c in '^_' and s[i+1:i+2]=='{':
            depth=0; j=i+1
            while True:
                if s[j]=='{': depth+=1
                elif s[j]=='}':
                    depth-=1
                    if depth==0: break
                j+=1
            out.append(c+'('+walk(s[i+2:j])+')'); i=j+1; continue
        if c=='⟪':
            j=s.index('⟫',i); w=s[i+1:j]
            if ':' in w:
                mark,letter=w.split(':',1)
                out.append('arrow(arrow(%s))'%walk(letter) if mark=='dyad' else '%s(%s)'%(mark,walk(letter))); i=j+1; continue
            if w=='pre':
                last=next((t for t in reversed(out) if t.strip()),'')
                out.append(('#h(0.15em)' if last[-1:].isalnum() or last[-1:]==')' else '')+'""'); i=j+1; continue
            out.append({'xbar':'overline(x)','ybar':'overline(y)','angle':'angle.l N angle.r','dots':'dots','FS':'"F.S."','mdot':'dot(m)','Qdot':'dot(Q)','Wdot':'dot(W)','Ibar':'overline(I)','yhat':'hat(y)'}[w]); i=j+1; continue
        if c=='√':
            if s[i+1:i+2]=='(':
                out.append('sqrt'); i+=1; continue
            j=i+1
            while j<len(s) and s[j].isalnum(): j+=1
            out.append('sqrt('+walk(s[i+1:j])+')'); i=j; continue
        big={'Σ':'sum','Π':'product','∫':'integral','∮':'integral.cont','∬':'integral.double','∭':'integral.triple','∯':'integral.surf'}
        if c in big:
            out.append(big[c]); i+=1; continue
        if c.isalpha():
            j=i
            while j<len(s) and s[j].isalpha() and s[j] not in big: j+=1
            w=s[i:j]
            if UNITS:
                out.append('"'+w+'"'); i=j; continue
            if w=='atan' and s[j:j+1]=='2':
                out.append('op("atan2")'); i=j+1; continue
            if w=='proj': out.append('op("proj")')
            elif w in OPS: out.append(w)
            elif w in UPRIGHT or (len(w)>=4 and w.isascii()) : out.append('"'+w+'"')
            elif len(w)==1: out.append(w)
            else: out.append('\\'.join(w))
            i=j; continue
        if UNITS and c=='/': out.append('\\/'); i+=1; continue
        # a unit keeps its space from the number, even with a power on it; operators space themselves
        if UNITS and c==' ':
            prev=s[i-1:i]; nxt=s[i+1:i+2]
            out.append(' ' if prev in '=×+−·' or nxt in '=×+−·' else '#h(0.28em)'); i+=1; continue
        if c=='−': out.append('-'); i+=1; continue
        if c=='≤': out.append('<='); i+=1; continue
        if c=='≥': out.append('>='); i+=1; continue
        if c=='∗': out.append('*'); i+=1; continue
        if c=='%': out.append('\\%'); i+=1; continue
        if c in '#$\\"': out.append('\\'+c); i+=1; continue
        out.append(c); i+=1
    return ''.join(out)
def note_doc(n):
    "a note with maths in it comes as maths; the rest is words"
    n2=n.replace('ωₒ','ω_{o}')
    head=None
    for sep in [', ']:
        pass
    if re.search(r'[=<>≤]',n2) and not re.search(r'[A-Za-z]{4,}',n2):
        return {'math':body(n2)}
    return {'text':n}
NAMED=[
 (["Ohm's law","ohm law"],['V = I R']),
 (["Newton's 2nd law","newton's 2nd","newton's second law","newton's second","newton 2nd law","f = ma","f=ma"],['F = m a']),
 (["Newton's law of gravity","newton's gravity","law of gravitation","universal gravitation"],['F = G m_{1} m_{2}/r^{2}']),
 (["Newton's law of cooling","law of cooling"],['Q̇ = h A (T − T_{∞})']),
 (["KCL","Kirchhoff's current law"],['Σ I = 0']),
 (["KVL","Kirchhoff's voltage law"],['Σ V = 0']),
 (["Kirchhoff's laws"],['Σ I = 0','Σ V = 0']),
 (["voltage divider"],['V_{out} = V_{in} R_{2}/(R_{1} + R_{2})']),
 (["current divider"],['I_{1} = I R_{2}/(R_{1} + R_{2})']),
 (["Wheatstone bridge","wheatstone"],['V_{out} = V_{s} (R_{2}/(R_{1} + R_{2}) − R_{4}/(R_{3} + R_{4}))']),
 (["Thévenin","thevenin","norton"],['V_{th} = V_{oc}','R_{th} = V_{oc}/I_{sc}','I_{N} = I_{sc}']),
 (["Hooke's law","hooke"],['F = −k x','σ = E ε']),
 (["Snell's law","snell"],['n_{1} sin(θ_{1}) = n_{2} sin(θ_{2})']),
 (["Coulomb's law","coulomb's"],['F = k q_{1} q_{2}/r^{2}']),
 (["Faraday's law","faraday's"],['ε = −N dΦ/dt']),
 (["Gauss's law","gauss law"],['∇ · E = ρ/ε_{0}']),
 (["Ampère's law","ampere's law"],['∇ × B = μ_{0} J + μ_{0} ε_{0} ∂E/∂t']),
 (["Maxwell's equations","maxwell"],['∇ · E = ρ/ε_{0}','∇ · B = 0','∇ × E = −∂B/∂t','∇ × B = μ_{0} J + μ_{0} ε_{0} ∂E/∂t']),
 (["Lorentz force","lorentz"],['F = q(E + v × B)']),
 (["Bernoulli's equation","bernoulli","bernoulli's principle"],['p + ρv^{2}/2 + ρgz = constant']),
 (["Poiseuille's law","poiseuille"],['Q = πR^{4}Δp/(8μL)']),
 (["Reynolds number"],['Re = ρvD/μ']),
 (["Navier-Stokes","navier stokes"],['ρ Dv/Dt = −∇p + μ∇^{2}v + ρg']),
 (["ideal gas law","pv = nrt","pv=nrt"],['P V = n R T']),
 (["combined gas law"],['P_{1} V_{1}/T_{1} = P_{2} V_{2}/T_{2}']),
 (["Boyle's law","boyle"],['P_{1} V_{1} = P_{2} V_{2}']),
 (["Charles's law","charles law","charles' law"],['V_{1}/T_{1} = V_{2}/T_{2}']),
 (["Gay-Lussac's law","gay lussac","gay-lussac"],['P_{1}/T_{1} = P_{2}/T_{2}']),
 (["ratio test"],['L = lim_{n→∞} |a_{n+1}/a_{n}|']),
 (["van der Waals"],['(P + a n^{2}/V^{2})(V − n b) = n R T']),
 (["Graham's law"],['r_{1}/r_{2} = √(M_{2}/M_{1})']),
 (["Dalton's law"],['P_{tot} = Σ P_{i}']),
 (["Beer-Lambert law","beer's law","beer lambert"],['A = ε l c']),
 (["dilution","c1v1 = c2v2","c1v1=c2v2"],['c_{1} V_{1} = c_{2} V_{2}']),
 (["Hess's law"],['ΔH° = Σ ΔH°_{f}(products) − Σ ΔH°_{f}(reactants)']),
 (["Gibbs free energy","gibbs"],['ΔG = ΔH − T ΔS','ΔG° = −R T ln(K)']),
 (["Arrhenius equation","arrhenius"],['k = A e^{−E_{a}/(RT)}']),
 (["Nernst equation","nernst"],['E = E° − (RT/(nF)) ln(Q)','E = E° − (0.0592/n) log_{10}(Q)','E = (RT/(zF)) ln([X]_{out}/[X]_{in})']),
 (["Henderson-Hasselbalch","henderson hasselbalch"],['pH = pK_{a} + log_{10}([A^{−}]/[HA])']),
 (["Raoult's law"],['P = x P°']),
 (["Rydberg equation","rydberg"],['1/λ = R_{H} (1/n_{1}^{2} − 1/n_{2}^{2})']),
 (["Clausius-Clapeyron","clausius clapeyron"],['ln(P_{2}/P_{1}) = −(ΔH_{vap}/R)(1/T_{2} − 1/T_{1})']),
 (["Michaelis-Menten","michaelis menten"],['v = V_{max} [S]/(K_{m} + [S])']),
 (["Lineweaver-Burk","lineweaver burk"],['1/v = (K_{m}/V_{max})(1/[S]) + 1/V_{max}']),
 (["Hardy-Weinberg","hardy weinberg"],['p + q = 1','p^{2} + 2pq + q^{2} = 1']),
 (["Fick's law"],['J = −D dC/dx']),
 (["logistic growth"],['dN/dt = r N (1 − N/K)']),
 (["chi-square","chi square"],['χ^{2} = Σ (O − E)^{2}/E']),
 (["cardiac output"],['CO = HR × SV']),
 (["Euler's formula"],['e^{jθ} = cos(θ) + j sin(θ)']),
 (["de Moivre's theorem","de moivre"],['(cos θ + j sin θ)^{n} = cos(nθ) + j sin(nθ)']),
 (["Pythagorean theorem","pythagoras"],['a^{2} + b^{2} = c^{2}']),
 (["law of cosines"],['c^{2} = a^{2} + b^{2} − 2ab cos(C)']),
 (["law of sines"],['a/sin(A) = b/sin(B) = c/sin(C)']),
 (["Heron's formula"],['A = √(s(s − a)(s − b)(s − c))']),
 (["quadratic formula"],['x = (−b ± √(b^{2} − 4ac))/(2a)']),
 (["Bayes' theorem","bayes theorem","bayes"],['P(A | B) = P(B | A) P(A)/P(B)']),
 (["z-score","z score"],['z = (x − μ)/σ']),
 (["standard deviation"],['s = √(Σ (x_{i} − x̄)^{2}/(n − 1))','s^{2} = Σ (x_{i} − x̄)^{2} / (n − 1)']),
 (["Euler buckling","buckling","critical buckling stress","slenderness ratio"],['P_{cr} = π^{2}EI/(KL)^{2}','σ_{cr} = π^{2}E/(KL/r)^{2}','r = √(I/A)']),
 (["Carnot efficiency","carnot"],['η = 1 − T_{c}/T_{h}']),
 (["Stokes' theorem","stokes theorem"],['∮_{C} F · dr = ∬_{S} (∇ × F) · dS']),
 (["Green's theorem"],['∮_{C} (P dx + Q dy) = ∬_{D} (∂Q/∂x − ∂P/∂y) dA']),
 (["divergence theorem"],['∯_{S} F · dS = ∭_{V} (∇ · F) dV']),
 (["Nyquist rate","nyquist theorem"],['f_{s} ≥ 2 f_{max}']),
 (["Shannon capacity","shannon hartley"],['C = B log_{2}(1 + S/N)']),
 (["Friis equation"],['P_{r} = P_{t} G_{t} G_{r} (λ/(4πd))^{2}']),
 (["Shockley diode equation","diode equation","shockley"],['I = I_{S} (e^{V/(nV_{T})} − 1)']),
 (["mass-energy","e = mc^2","e=mc^2","e=mc2"],['E = m c^{2}']),
 (["de Broglie wavelength","de broglie"],['λ = h/p']),
 (["Schrödinger equation","schrodinger equation","schrodinger"],['iħ ∂ψ/∂t = H ψ']),
 (["Heisenberg uncertainty","uncertainty principle"],['Δx Δp ≥ ħ/2']),
 (["photoelectric effect"],['KE = h f − φ']),
 (["time dilation"],['t = γ t_{0}']),
 (["length contraction"],['L = L_{0}/γ']),
 (["half-life","half life"],['t_{1/2} = ln(2)/λ']),
 (["radioactive decay"],['N = N_{0} e^{−λt}']),
 (["kinetic energy"],['KE = (1/2) m v^{2}']),
 (["potential energy"],['PE = m g h']),
 (["von Mises","von mises stress"],['σ_{v} = √(((σ_{1} − σ_{2})^{2} + (σ_{2} − σ_{3})^{2} + (σ_{3} − σ_{1})^{2})/2)','σ_{v} = √(σ^{2} + 3τ^{2})']),
 (["Tresca"],['τ_{max} = (σ_{1} − σ_{3})/2']),
 (["flexure formula","bending stress"],['σ = −My/I','σ_{max} = Mc/I = M/S']),
 (["torsion formula"],['τ = Tρ/J','τ_{max} = Tc/J','φ = TL/(GJ)']),
 (["shear formula","beam shear stress"],['τ = VQ/(It)']),
 (["hoop stress"],['σ_{h} = pr/t']),
 (["Reynolds transport theorem"],['dB/dt = d/dt ∫_{CV} ρb dV + ∫_{CS} ρb (v · n) dA']),
 (["heat equation"],['∂u/∂t = α ∂^{2}u/∂x^{2}']),
 (["wave equation"],['∂^{2}u/∂t^{2} = c^{2} ∂^{2}u/∂x^{2}']),
 (["integrating factor"],['μ = e^{∫ p dt}','y = (1/μ)(∫ μ q dt + C)']),
 (["Newton's method","newton raphson","newton-raphson"],['x_{n+1} = x_{n} − f(x_{n})/f′(x_{n})']),
 (["Simpson's rule"],['∫ f dx ≈ (h/3)(f_{0} + 4f_{1} + 2f_{2} + 4f_{3} + … + f_{n})']),
 (["trapezoid rule","trapezoidal rule"],['∫ f dx ≈ (h/2)(f_{0} + 2f_{1} + 2f_{2} + … + f_{n})']),
 (["Runge-Kutta","rk4","runge kutta"],['k_{1} = h f(t_{n}, y_{n})','k_{2} = h f(t_{n} + h/2, y_{n} + k_{1}/2)','k_{3} = h f(t_{n} + h/2, y_{n} + k_{2}/2)','k_{4} = h f(t_{n} + h, y_{n} + k_{3})','y_{n+1} = y_{n} + (k_{1} + 2k_{2} + 2k_{3} + k_{4})/6']),
 (["master theorem"],['T(n) = a T(n/b) + f(n)','T(n) = Θ(n^{log_{b} a})','T(n) = Θ(n^{log_{b} a} log n)','T(n) = Θ(f(n))']),
 (["Amdahl's law"],['S = 1/((1 − p) + p/s)']),
 (["gradient descent"],['w ← w − η ∇L']),
 (["sigmoid"],['σ(z) = 1/(1 + e^{−z})']),
 (["softmax"],['p_{i} = e^{z_{i}}/(Σ_{j} e^{z_{j}})']),
 (["cross-entropy","cross entropy"],['L = −Σ y_{i} ln(ŷ_{i})']),
 (["F1 score"],['F_{1} = 2PR/(P + R)']),
 (["Kepler's third law","kepler"],['T^{2} = 4π^{2} a^{3}/(G M)']),
 (["escape velocity"],['v_{e} = √(2 G M/r)']),
 (["Hubble's law"],['v = H_{0} d']),
 (["Schwarzschild radius"],['r_{s} = 2 G M/c^{2}']),
 (["Stefan-Boltzmann","stefan boltzmann"],['L = 4π R^{2} σ T^{4}','Q̇ = ε σ A (T_{1}^{4} − T_{2}^{4})']),
 (["Wien's law"],['λ_{max} = b/T']),
 (["compound interest","continuous compounding"],['A = P(1 + r/n)^{n t}','A = P e^{r t}']),
 (["simple interest"],['A = P(1 + r t)']),
 (["loan payment","mortgage payment"],['M = P r/(1 − (1 + r)^{−n})']),
 (["net present value","npv"],['NPV = Σ C_{t}/(1 + r)^{t}']),
 (["rule of 72"],['t ≈ 72/r']),
 (["internal rate of return","irr"],['Σ C_{t}/(1 + IRR)^{t} = 0','NPV = Σ C_{t}/(1 + r)^{t}']),
 (["annuity due"],['PV_{due} = (1 + r) PMT (1 − (1 + r)^{−n})/r','PV = PMT (1 − (1 + r)^{−n})/r']),
 (["payback period"],['PP = I/C']),
 (["portfolio variance","portfolio return"],['E[R_{p}] = Σ w_{i} E[R_{i}]','σ_{p}^{2} = w_{1}^{2}σ_{1}^{2} + w_{2}^{2}σ_{2}^{2} + 2w_{1}w_{2}ρσ_{1}σ_{2}']),
 (["declining balance depreciation","depreciation"],['D = (C − S)/n','B_{k} = C (1 − d)^{k}']),
 (["empirical rule","68-95-99.7 rule"],['within 1σ: 68.27%','within 2σ: 95.45%','within 3σ: 99.73%']),
 (["two-sample t test","two sample t test"],['t = (x̄_{1} − x̄_{2})/√(s_{1}^{2}/n_{1} + s_{2}^{2}/n_{2})']),
 (["hypothesis test","t test","z test"],['z = (x̄ − μ_{0})/(σ/√n)','t = (x̄ − μ_{0})/(s/√n)','z = (p̂ − p_{0})/√(p_{0}(1 − p_{0})/n)','t = (x̄_{1} − x̄_{2})/√(s_{1}^{2}/n_{1} + s_{2}^{2}/n_{2})']),
 (["confidence interval"],['x̄ ± z σ/√n','x̄ ± t s/√n','p̂ ± z √(p̂(1 − p̂)/n)']),
 (["interquartile range","iqr"],['IQR = Q_{3} − Q_{1}']),
 (["frustum"],['V = (1/3) πh(R^{2} + R r + r^{2})']),
 (["similar figures","similar triangles"],['A_{2}/A_{1} = k^{2},  V_{2}/V_{1} = k^{3}']),
 (["inscribed angle theorem","inscribed angle"],['θ_{i} = θ_{c}/2']),
 (["cache organization","cache address bits"],['S = C/(B k)','t = a − log_{2}(S) − log_{2}(B)','AMAT = t_{hit} + m t_{miss}']),
 (["stop-and-wait","stop and wait"],['U = (L/R)/(RTT + L/R)']),
 (["naive Bayes","naive bayes classifier"],['ŷ = argmax_{y} P(y) Π_{i} P(x_{i} | y)']),
 (["k-means","k means clustering"],['J = Σ_{i} ‖x_{i} − μ_{c_{i}}‖^{2}']),
 (["SVM margin","support vector machine"],['margin = 2/‖w‖','L = max(0, 1 − y ŷ)']),
 (["convolution output size","conv output size"],['n_{out} = (n + 2p − k)/s + 1']),
 (["bias-variance tradeoff","bias variance"],['E[(y − ŷ)^{2}] = Bias^{2} + Var + σ^{2}']),
 (["decision tree","gini impurity","information gain"],['G = 1 − Σ p_{i}^{2}','H = −Σ p_{i} log_{2}(p_{i})','IG = H(parent) − Σ (n_{k}/n) H(child_{k})']),
 # ---- precalculus to continuum mechanics, and statics and dynamics: what a student types for a formula
 (["distance formula"],['d = √((x_{2} − x_{1})^{2} + (y_{2} − y_{1})^{2})']),
 (["midpoint formula"],['((x_{1} + x_{2})/2, (y_{1} + y_{2})/2)']),
 (["slope formula"],['m = (y_{2} − y_{1})/(x_{2} − x_{1})']),
 (["point-slope form","point slope form","point slope"],['y − y_{1} = m(x − x_{1})']),
 (["slope-intercept form","slope intercept form","slope intercept"],['y = m x + b']),
 (["completing the square"],['a x^{2} + b x + c = a(x + b/(2a))^{2} + c − b^{2}/(4a)','y = a(x − h)^{2} + k']),
 (["Vieta's formulas","vieta"],['r_{1} + r_{2} = −b/a,  r_{1} r_{2} = c/a']),
 (["difference of squares"],['a^{2} − b^{2} = (a + b)(a − b)']),
 (["sum of cubes","difference of cubes"],['a^{3} + b^{3} = (a + b)(a^{2} − ab + b^{2})','a^{3} − b^{3} = (a − b)(a^{2} + ab + b^{2})']),
 (["binomial theorem"],['(a + b)^{n} = Σ_{k=0}^{n} C(n, k) a^{n−k}b^{k}']),
 (["remainder theorem","factor theorem"],['p(x) = (x − r) q(x) + p(r)']),
 (["log rules","logarithm rules","laws of logarithms"],['log_{b}(x) = y ⇔ b^{y} = x','log_{b}(xy) = log_{b}(x) + log_{b}(y)','log_{b}(x/y) = log_{b}(x) − log_{b}(y)','log_{b}(x^{n}) = n log_{b}(x)','log_{b}(x) = ln(x)/ln(b)']),
 (["change of base"],['log_{b}(x) = ln(x)/ln(b)']),
 (["exponent rules","laws of exponents"],['a^{m}a^{n} = a^{m+n}','a^{m}/a^{n} = a^{m−n}','(a^{m})^{n} = a^{mn}','a^{−n} = 1/a^{n}','a^{1/n} = ^{n}√a']),
 (["function transformations","graph transformations"],['y = a f(b(x − h)) + k']),
 (["even and odd functions"],['f(−x) = f(x)','f(−x) = −f(x)']),
 (["arithmetic sequence","arithmetic series"],['a_{n} = a_{1} + (n − 1) d','S_{n} = n(a_{1} + a_{n})/2']),
 (["geometric sequence","geometric series"],['a_{n} = a_{1} r^{n−1}','S_{n} = a_{1}(1 − r^{n})/(1 − r)','S = a_{1}/(1 − r)']),
 (["Bernoulli differential equation","bernoulli ode"],['dy/dt + p y = q y^{n}']),
 (["exact equation","exact differential equation"],['M dx + N dy = 0']),
 (["characteristic equation"],['a r^{2} + b r + c = 0','det(A − λI) = 0']),
 (["Wronskian"],['W = y_{1} y′_{2} − y_{2} y′_{1}']),
 (["variation of parameters"],['y_{p} = −y_{1} ∫ y_{2} g/W dt + y_{2} ∫ y_{1} g/W dt']),
 (["undetermined coefficients"],['A_{n} t^{n} + … + A_{1} t + A_{0}','A e^{at}','A cos(ωt) + B sin(ωt)','e^{at} (A cos(ωt) + B sin(ωt))']),
 (["Euler-Cauchy","cauchy-euler","euler cauchy"],['a t^{2} y′′ + b t y′ + c y = 0','a r(r − 1) + b r + c = 0']),
 (["separation of variables"],['∫ dy/g(y) = ∫ f(t) dt','u = X(x) T(t)']),
 (["d'Alembert's solution","d'alembert"],['u = f(x − ct) + g(x + ct)']),
 (["Laplace's equation","laplace equation"],['∂^{2}u/∂x^{2} + ∂^{2}u/∂y^{2} = 0']),
 (["bisection method","bisection"],['c = (a + b)/2','|e_{n}| ≤ (b − a)/2^{n}']),
 (["secant method"],['x_{n+1} = x_{n} − f(x_{n})(x_{n} − x_{n−1})/(f(x_{n}) − f(x_{n−1}))']),
 (["fixed-point iteration","fixed point iteration"],['x_{n+1} = g(x_{n})']),
 (["linear interpolation"],['y = y_{0} + (y_{1} − y_{0})(x − x_{0})/(x_{1} − x_{0})']),
 (["Lagrange interpolation","lagrange polynomial"],['p(x) = Σ_{i=0}^{n} y_{i} Π_{j≠i} (x − x_{j})/(x_{i} − x_{j})']),
 (["finite differences","finite difference","central difference"],['f′(x) ≈ (f(x + h) − f(x))/h','f′(x) ≈ (f(x) − f(x − h))/h','f′(x) ≈ (f(x + h) − f(x − h))/(2h)','f′′(x) ≈ (f(x + h) − 2f(x) + f(x − h))/h^{2}']),
 (["midpoint rule"],['∫ f dx ≈ h Σ_{i=0}^{n−1} f(x_{i} + h/2)']),
 (["Euler's method","euler method","forward euler"],['y_{n+1} = y_{n} + h f(t_{n}, y_{n})']),
 (["backward Euler","implicit euler"],['y_{n+1} = y_{n} + h f(t_{n+1}, y_{n+1})']),
 (["Heun's method","improved euler"],['y_{n+1} = y_{n} + (h/2)(f(t_{n}, y_{n}) + f(t_{n+1}, y_{n} + h f(t_{n}, y_{n})))']),
 (["LU decomposition","lu factorization"],['A = L U']),
 (["Jacobi iteration","jacobi method"],['x_{i}^{(k+1)} = (b_{i} − Σ_{j≠i} a_{ij} x_{j}^{(k)})/a_{ii}']),
 (["power method","power iteration"],['x_{k+1} = A x_{k}/‖A x_{k}‖']),
 (["condition number"],['κ(A) = ‖A‖ ‖A^{−1}‖']),
 (["least squares","normal equation"],['x = (A^{T} A)^{−1} A^{T} b','w = (X^{T} X)^{−1} X^{T} y']),
 (["Fourier's law","fourier's law of conduction"],['Q̇ = −k A dT/dx','Q̇ = k A (T_{1} − T_{2})/L']),
 (["LMTD","log mean temperature difference"],['ΔT_{m} = (ΔT_{1} − ΔT_{2})/ln(ΔT_{1}/ΔT_{2})','Q̇ = U A ΔT_{m}']),
 (["effectiveness-NTU","ntu method","e-ntu"],['NTU = U A/C_{min}','ε = Q̇/Q̇_{max}','Q̇_{max} = C_{min} (T_{h,in} − T_{c,in})','ε = (1 − e^{−NTU(1 − c)})/(1 − c e^{−NTU(1 − c)})']),
 (["Biot number"],['Bi = h L/k']),
 (["Nusselt number"],['Nu = h L/k']),
 (["Prandtl number"],['Pr = μ c_{p}/k']),
 (["Grashof number"],['Gr = g β (T_{s} − T_{∞}) L^{3}/ν^{2}']),
 (["Rayleigh number"],['Ra = Gr Pr']),
 (["Fourier number"],['Fo = α t/L^{2}']),
 (["Dittus-Boelter","dittus boelter"],['Nu = 0.023 Re^{0.8} Pr^{n}']),
 (["lumped capacitance","lumped capacity"],['T − T_{∞} = (T_{0} − T_{∞}) e^{−t/τ}','τ = ρ V c_{p}/(h A)','Bi = h L/k']),
 (["fin equation","fin efficiency"],['m = √(h P/(k A_{c}))','Q̇ = √(h P k A_{c}) (T_{b} − T_{∞}) tanh(m L)','η_{f} = tanh(m L)/(m L)']),
 (["critical radius of insulation","critical radius"],['r_{cr} = k/h']),
 (["view factor"],['A_{1} F_{12} = A_{2} F_{21}','Σ_{j} F_{ij} = 1']),
 (["Little's law"],['L = λ W']),
 (["Gustafson's law"],['S = (1 − p) + p N']),
 (["two's complement"],['−2^{n−1} ≤ x ≤ 2^{n−1} − 1','0 ≤ x ≤ 2^{n} − 1']),
 (["merge sort"],['T(n) = 2T(n/2) + Θ(n) = Θ(n log n)']),
 (["binary search"],['T(n) = T(n/2) + Θ(1) = Θ(log n)']),
 (["handshake lemma"],['Σ deg(v) = 2E']),
 (["birthday problem","birthday paradox"],['P ≈ 1 − e^{−n^{2}/(2m)}']),
 (["Shannon entropy","information entropy"],['H = −Σ p_{i} log_{2}(p_{i})']),
 (["linear regression"],['ŷ = w · x + b','w = (X^{T} X)^{−1} X^{T} y']),
 (["ridge regression"],['w = (X^{T} X + λI)^{−1} X^{T} y','L + λ ‖w‖^{2}']),
 (["lasso regression","lasso"],['L + λ ‖w‖_{1}']),
 (["logistic regression"],['ŷ = σ(w · x + b)','L = −(y ln(ŷ) + (1 − y) ln(1 − ŷ))']),
 (["ReLU"],['ReLU(z) = max(0, z)']),
 (["mean squared error","root mean squared error","rmse"],['MSE = (1/n) Σ (y_{i} − ŷ_{i})^{2}','RMSE = √(MSE)']),
 (["r squared","r-squared","coefficient of determination"],['R^{2} = 1 − SS_{res}/SS_{tot}']),
 (["binary cross-entropy","binary cross entropy","log loss"],['L = −(y ln(ŷ) + (1 − y) ln(1 − ŷ))']),
 (["hinge loss"],['L = max(0, 1 − y ŷ)']),
 (["KL divergence","kullback-leibler"],['D = Σ p_{i} ln(p_{i}/q_{i})']),
 (["Gini impurity"],['G = 1 − Σ p_{i}^{2}']),
 (["information gain"],['IG = H(parent) − Σ (n_{k}/n) H(child_{k})']),
 (["attention mechanism","scaled dot-product attention","self-attention"],['A = softmax(Q K^{T}/√(d_{k})) V']),
 (["Adam optimizer"],['w ← w − η m̂/(√(v̂) + ϵ)']),
 (["confusion matrix","precision and recall"],['P = TP/(TP + FP)','R = TP/(TP + FN)','TNR = TN/(TN + FP)','FPR = FP/(FP + TN)','F_{1} = 2PR/(P + R)','A = (TP + TN)/(TP + TN + FP + FN)']),
 (["cosine similarity"],['cos(θ) = (a · b)/(|a| |b|)']),
 (["vis-viva","vis viva"],['v^{2} = G M (2/r − 1/a)']),
 (["distance modulus"],['m − M = 5 log_{10}(d) − 5']),
 (["apparent magnitude"],['m_{1} − m_{2} = −2.5 log_{10}(F_{1}/F_{2})','m − M = 5 log_{10}(d) − 5']),
 (["parallax"],['d = 1/p']),
 (["redshift"],['z = Δλ/λ','1 + z = λ_{obs}/λ_{emit}','v = c z']),
 (["small-angle formula","small angle formula"],['θ = D/d']),
 (["mass-luminosity relation","mass luminosity"],['L/L_{sun} = (M/M_{sun})^{3.5}']),
 (["Rayleigh criterion","diffraction limit"],['θ = 1.22 λ/D']),
 (["present value"],['PV = FV/(1 + r)^{n}','PV = FV e^{−r t}','PV = PMT (1 − (1 + r)^{−n})/r']),
 (["future value"],['FV = PV (1 + r)^{n}','FV = PMT ((1 + r)^{n} − 1)/r']),
 (["annuity"],['PV = PMT (1 − (1 + r)^{−n})/r','FV = PMT ((1 + r)^{n} − 1)/r']),
 (["perpetuity","gordon growth model"],['PV = C/r','PV = C/(r − g)']),
 (["effective annual rate","apy"],['EAR = (1 + r/n)^{n} − 1']),
 (["amortization","loan balance"],['M = P r/(1 − (1 + r)^{−n})','B = P (1 + r)^{k} − M ((1 + r)^{k} − 1)/r']),
 (["CAPM","capital asset pricing model"],['E[R] = r_{f} + β (E[R_{m}] − r_{f})']),
 (["Sharpe ratio"],['S = (R_{p} − r_{f})/σ_{p}']),
 (["WACC","weighted average cost of capital"],['WACC = (E/V) r_{E} + (D/V) r_{D} (1 − T)']),
 (["bond price","bond pricing"],['P = Σ_{t=1}^{n} C/(1 + y)^{t} + F/(1 + y)^{n}']),
 (["CAGR"],['CAGR = (V_{f}/V_{i})^{1/n} − 1']),
 (["real interest rate","fisher equation"],['r_{real} = (1 + r)/(1 + i) − 1']),
 (["van 't Hoff equation","van't hoff","van t hoff"],["ln(K_{2}/K_{1}) = −(ΔH°/R)(1/T_{2} − 1/T_{1})"]),
 (["Henry's law"],['c = k_{H} P']),
 (["Bohr model"],['E_{n} = −13.6 eV/n^{2}','1/λ = R_{H} (1/n_{1}^{2} − 1/n_{2}^{2})']),
 (["boiling point elevation","freezing point depression"],['ΔT_{b} = i K_{b} b','ΔT_{f} = i K_{f} b']),
 (["electrolysis","faraday's law of electrolysis"],['m = I t M/(n F)']),
 (["mark-recapture","lincoln-petersen","lincoln index"],['N = M C/R']),
 (["Lotka-Volterra","predator-prey","lotka volterra"],['dN/dt = r N − a N P','dP/dt = b N P − m P']),
 (["Goldman equation","ghk equation"],['V_{m} = (RT/F) ln((P_{K}[K^{+}]_{o} + P_{Na}[Na^{+}]_{o} + P_{Cl}[Cl^{−}]_{i})/(P_{K}[K^{+}]_{i} + P_{Na}[Na^{+}]_{i} + P_{Cl}[Cl^{−}]_{o}))']),
 (["Hill equation"],['θ = [L]^{n}/(K_{A}^{n} + [L]^{n})']),
 (["Shannon diversity index","shannon index"],['H = −Σ p_{i} ln(p_{i})']),
 (["Simpson's index","simpson diversity index"],['D = 1 − Σ p_{i}^{2}']),
 (["water potential"],['Ψ = Ψ_{s} + Ψ_{p}','Ψ_{s} = −i C R T']),
 (["exponential growth"],['y = y_{0} e^{kt}','N = N_{0} e^{rt}','dN/dt = r N']),
 (["doubling time"],['t_{d} = ln(2)/r','t ≈ 72/r']),
 (["heritability","breeder's equation"],['h^{2} = V_{A}/V_{P}','R = h^{2} S']),
 (["pharmacokinetics","drug clearance","loading dose","volume of distribution"],['C = C_{0} e^{−kt}','t_{1/2} = ln(2)/k','V_{d} = dose/C_{0}','CL = k V_{d}','LD = C_{target} V_{d}']),
 (["radius of gyration"],['r = √(I/A)','k = √(I/m)']),
 (["continuity equation"],['A_{1} v_{1} = A_{2} v_{2}','ṁ = ρ_{1} A_{1} v_{1} = ρ_{2} A_{2} v_{2}','∂ρ/∂t + ∇ · (ρv) = 0']),
 (["Torricelli's law","torricelli"],['v = √(2gh)']),
 (["drag equation","drag force"],['F_{D} = (1/2) C_{D} ρ v^{2} A']),
 (["lift equation","lift force"],['F_{L} = (1/2) C_{L} ρ v^{2} A']),
 (["Darcy-Weisbach","darcy weisbach","head loss"],['h_{f} = f (L/D) v^{2}/(2g)','f = 64/Re','h_{m} = K v^{2}/(2g)']),
 (["buoyancy","archimedes principle","archimedes' principle"],['F_{b} = ρgV']),
 (["elastic moduli relations","shear modulus","bulk modulus"],['G = E/(2(1 + ν))','K = E/(3(1 − 2ν))','λ = Eν/((1 + ν)(1 − 2ν))']),
 (["thermal stress"],['σ = −EαΔT','δ_{T} = αΔT L']),
 (["polar moment of inertia"],['J = πd^{4}/32','J = π(d_{o}^{4} − d_{i}^{4})/32']),
 (["second moment of area","area moment of inertia"],['I = bh^{3}/12','I = πd^{4}/64','I = π(d_{o}^{4} − d_{i}^{4})/64','I = Ī + Ad^{2}']),
 (["parallel axis theorem"],['I = Ī + m d^{2}','I = Ī + Ad^{2}']),
 (["Coriolis acceleration","coriolis"],['^{N}a⃗^{Q} = ^{N}a⃗^{B_{o}} + ^{N}α⃗^{B} × r⃗ + ^{N}ω⃗^{B} × (^{N}ω⃗^{B} × r⃗) + 2 ^{N}ω⃗^{B} × ^{B}v⃗^{Q} + ^{B}a⃗^{Q}']),
 (["Euler's equations","euler equations of motion"],['M_{1} = I_{1} ω̇_{1} − (I_{2} − I_{3}) ω_{2} ω_{3}']),
 (["Lagrange's equations","lagrange equation"],['−∂U/∂q_{r} = d(∂K/∂q̇_{r})/dt − ∂K/∂q_{r}']),
 (["work-energy theorem","work energy theorem"],['W = Δ KE','W̃ = ΔK + ΔU']),
 (["impulse-momentum","impulse momentum"],['∫ F dt = Δp','∫ F⃗ dt = ΔL⃗']),
 (["conservation of momentum"],['m_{1} v_{1} + m_{2} v_{2} = m_{1} v′_{1} + m_{2} v′_{2}']),
 (["coefficient of restitution","restitution"],['e = (v′_{2} − v′_{1})/(v_{1} − v_{2})']),
 (["natural frequency"],['ω_{n} = √(k/m)','f_{n} = ω_{n}/(2π)']),
 (["damping ratio","critical damping"],['c_{c} = 2√(k m)','ζ = c/c_{c}','ω_{d} = ω_{n} √(1 − ζ^{2})']),
 (["logarithmic decrement","log decrement"],['δ = ln(x_{1}/x_{2})','ζ = δ/√(4π^{2} + δ^{2})']),
 (["springs in series","springs in parallel"],['k_{eq} = k_{1} + k_{2}','1/k_{eq} = 1/k_{1} + 1/k_{2}']),
 (["forced vibration","magnification factor"],['m x′′ + c x′ + k x = F_{0} cos(ωt)','X = (F_{0}/k)/√((1 − r^{2})^{2} + (2ζr)^{2})','tan(φ) = 2ζr/(1 − r^{2})']),
 (["transmissibility","vibration isolation"],['TR = √(1 + (2ζr)^{2})/√((1 − r^{2})^{2} + (2ζr)^{2})']),
 (["capstan equation","belt friction"],['T_{2} = T_{1} e^{μβ}']),
 (["angle of repose","angle of friction"],['tan(φ) = μ']),
 (["centroid"],['x̄ = (Σ A_{i} x_{i})/(Σ A_{i})','x̄ = (∫ x dA)/(∫ dA)']),
 (["transport theorem","golden rule"],['^{A}dv⃗/dt = ^{B}dv⃗/dt + ^{A}ω⃗^{B} × v⃗']),
 (["vector triple product","bac-cab","bac cab"],['a⃗ × (b⃗ × c⃗) = b⃗ (a⃗ · c⃗) − c⃗ (a⃗ · b⃗)']),
 (["scalar triple product"],['a⃗ · b⃗ × c⃗ = b⃗ · c⃗ × a⃗ = c⃗ · a⃗ × b⃗']),
 (["rotation matrix"],['R_{x} = [1, 0, 0; 0, cos θ, −sin θ; 0, sin θ, cos θ]','R_{y} = [cos θ, 0, sin θ; 0, 1, 0; −sin θ, 0, cos θ]','R_{z} = [cos θ, −sin θ, 0; sin θ, cos θ, 0; 0, 0, 1]']),
]
def named_sheets(sheets):
    rows={}
    for sh in sheets:
        for sec in sh['sections']:
            for gr in sec['groups']:
                for it in gr['items']:
                    if it.get('figure') or not it.get('copy'): continue
                    rows.setdefault(it['show'],[]).append((sh,sec,it))
    out=[]
    for names,shows in NAMED:
        items=[]
        for show in shows:
            assert show in rows,show
            items.append(rows[show][0][2])
        title=names[0][0].upper()+names[0][1:]
        sec={'title':title,'wide':True}
        # a table's rows keep their columns
        if 'leadWidths' in rows[shows[0]][0][1]: sec['leadWidths']=rows[shows[0]][0][1]['leadWidths']
        sec['groups']=[{'title':rows[shows[0]][0][0]['title'],'items':items}]
        out.append({'id':'formula: '+names[0],'title':title,'hint':title,'names':names,'sections':[sec]})
    return out

for sh in native:
    for sec in sh['sections']:
        for g in sec['groups']:
            for it in g['items']:
                if it.get('figure'):
                    it['figure'],it['spots']=circle_figure(); continue
                if it.pop('words',False):
                    it['typst']=['"'+it['show']+'"']; continue
                pic=it.pop('pic',None)
                if pic:
                    it['typst']=[pic+' quad '+body(it['show'])]
                    # the picture alone, which a click on it shows enlarged
                    it['picture']=it.pop('picture',None) or pic
                    if it.get('note'):
                        d=note_doc(it['note'])
                        if 'math' in d: it['noteTypst']=d['math']
                    continue
                # the formulas page is words, not maths
                if it.get('open'): it['typst']=['"'+x+'"' for x in it['lead']+[it['show']]]
                else: it['typst']=[body(x) for x in it.get('lead',[])+[it['show']]]
                # a row's name is words too
                if it.get('leadCopy',['x'])[0]=='': it['typst'][0]='"'+it['lead'][0]+'"'
                if it.pop('unit',False): it['typst'][-1]=unit_body(it['show'])
                if it.get('note'):
                    d=note_doc(it['note'])
                    if 'math' in d: it['noteTypst']=d['math']
for sh in native:
    for sec in sh['sections']:
        for g in sec['groups']:
            for it in g['items']:
                for t in it.get('lead',[])+[it['show']]:
                    if it.get('figure'): continue
                    assert t.count('{')==t.count('}'),t
                    assert '—' not in t
# what the fraction parser can't do alone: a side has to hold the whole of a sum, and a prime goes before a subscript
for sh in native:
    for sec in sh['sections']:
        for gr in sec['groups']:
            for it in gr['items']:
                for t in it.get('typst',[]):
                    assert not re.search(r'/\((?:sum|integral)\)',t),t
                    assert not re.search(r'_\([^()]*\)\'',t),t
# search tags: words a sheet's search finds a row by that it doesn't show (macos/SheetSearch.swift), by the row's
# `show`; a row may also carry its own 'tags'. Add to TAGS anywhere above this loop.
TAGS=globals().get('TAGS',{})
TAGS.setdefault('V = I R','ohms law resistance')
# sheet search, round 3: words for rows its queries missed (projectile motion) or ranked a weaker row above
# (rotation matrix, lincoln index, effective length, weights update)
for k,v in {
 'R = v_{0}^{2} sin(2θ)/g':'projectile motion trajectory horizontal distance',
 'H = v_{0}^{2} sin^{2}(θ)/(2g)':'projectile motion trajectory peak apex',
 't = 2 v_{0} sin(θ)/g':'projectile motion trajectory hang time',
 'R_{x} = [1, 0, 0; 0, cos θ, −sin θ; 0, sin θ, cos θ]':'rotation matrix',
 'R_{y} = [cos θ, 0, sin θ; 0, 1, 0; −sin θ, 0, cos θ]':'rotation matrix',
 'R_{z} = [cos θ, −sin θ, 0; sin θ, cos θ, 0; 0, 0, 1]':'rotation matrix',
 'N = M C/R':'lincoln index',
 'P_{cr} = π^{2}EI/(KL)^{2}':'effective length',
 'E_{T} = −(b − a) h^{2} f′′(ξ)/12':'error bound',
 'w ← w − η ∇L':'weights update rule',
 'S = (R_{p} − r_{f})/σ_{p}':'risk adjusted return',
 'σ(z) = 1/(1 + e^{−z})':'logistic curve s-curve',
}.items():
    TAGS[k]=(TAGS.get(k,'')+' '+v).strip()
# sheet critic round 2 (geometry, statistics, finance, computer science, machine learning): spelled-out symbols,
# other names and half-typed phrases their queries missed
for k,v in {
 'A = πr^{2}':'pi r squared disc disk',
 'C = 2πr':'2 pi r perimeter of a circle diameter',
 'A = l w':'length times width',
 'A = (1/2) b h':'half base times height triangle area',
 'A = (1/2) a b sin(C)':'half ab sin c sas side angle side',
 'A = (√3/4) a^{2}':'root 3 over 4 sqrt 3',
 'V = l w h':'cuboid length width height box volume',
 'S = 2(l w + l h + w h)':'cuboid rectangular prism',
 'S = 4πr^{2}':'4 pi r squared sphere surface area',
 'σ_{x̄} = σ/√n':'sem',
 'Var(X + Y) = Var(X) + Var(Y) + 2σ_{XY}':'two random variables sum independent var(x+y)',
 'P(A ∪ B) = P(A) + P(B) − P(A ∩ B)':'union or either inclusion exclusion',
 'P(A ∩ B) = P(A) P(B)':'intersection both multiplication rule',
 'P(A | B) = P(A ∩ B)/P(B)':'given',
 'nCr = n!/(r!(n − r)!)':'choose n choose k',
 'np, np(1 − p)':'binomial distribution bernoulli trials successes',
 'λ, λ':'poisson distribution',
 '1/p, (1 − p)/p^{2}':'geometric distribution',
 '(a + b)/2, (b − a)^{2}/12':'uniform distribution',
 '1/λ, 1/λ^{2}':'exponential distribution memoryless waiting time',
 'μ, σ^{2}':'normal distribution gaussian bell curve normal pdf density',
 'x̄ ± z σ/√n':'ci',
 'p̂ ± z √(p̂(1 − p̂)/n)':'p hat phat sample proportion',
 'A = P e^{r t}':'pert',
 'EAR = (1 + r/n)^{n} − 1':'annual percentage yield',
 'Σ_{i=0}^{n} 2^{i} = 2^{n+1} − 1':'powers of two',
 'T(n) = 2T(n/2) + Θ(n) = Θ(n log n)':'mergesort',
 'E = 1/(1 − α)':'linear probing',
 'Σ deg(v) = 2E':'sum of degrees',
 'V − E + F = 2':'euler characteristic',
 'n = log_{2}(N)':'how many bits',
 'd = d_{proc} + d_{queue} + d_{trans} + d_{prop}':'nodal delay processing queuing',
 'ŷ = w · x + b':'y hat prediction',
 'w = (X^{T} X + λI)^{−1} X^{T} y':'tikhonov',
 'V = B h':'prism volume',
 'V = (4/3) πr^{3}':'sphere volume',
 'A + B + C = 180°':'angles in a triangle',
 '1 : 1 : √2':'isosceles right triangle',
 'E[X] = Σ x p(x)':'expectation mean of a random variable',
 'A = P(1 + r t)':'prt i = prt',
 'A = P(1 + r/n)^{n t}':'compounded monthly quarterly',
 'G = 1 − Σ p_{i}^{2}':'gini index',
 'A = softmax(Q K^{T}/√(d_{k})) V':'self-attention',
 'R^{2} = 1 − SS_{res}/SS_{tot}':'r-squared',
 # the formulas page: what a student calls these sheets
 'area, volume, triangles, circles, coordinates':'shapes',
 'probability, distributions, confidence intervals, hypothesis tests':'stats probability',
 'big O, recurrences, hashing, graphs, bits, caches, networks':'algorithms data structures cs',
 'regression, activations, neural networks, losses, optimizers, clustering, metrics':'neural networks deep learning ai ml activations',
 'interest, present value, annuities, loans, NPV, IRR, CAPM, bonds':'financial math money business',
}.items():
    TAGS[k]=(TAGS.get(k,'')+' '+v).strip()
# sheet critic round 2 (physics, circuits, heat transfer, continuum mechanics, chemistry, biology, astronomy): formulas
# typed without spaces, symbols spelled in parts (delta g, sigma 1), misspellings and course phrases their queries missed
for k,v in {
 'v = v_{0} + a t':'kinematic equations',
 'x = x_{0} + v_{0} t + (1/2) a t^{2}':'kinematic equations displacement',
 'v^{2} = v_{0}^{2} + 2a Δx':'kinematic equations timeless',
 'Δx = (1/2)(v_{0} + v) t':'kinematic equations',
 'F = m a':'f=ma fma',
 'PE = m g h':'mgh',
 'τ = I α':'angular acceleration',
 'F = G m_{1} m_{2}/r^{2}':'universal gravitation gravitational force',
 'F = −k x':'spring constant stiffness',
 'p = F/A':'force over area',
 'p = p_{0} + ρgh':'rho g h',
 'ṁ = ρ_{1} A_{1} v_{1} = ρ_{2} A_{2} v_{2}':'continuity equation',
 'f_{n} = n v/(2L)':'standing waves fundamental frequency',
 'f_{n} = n v/(4L)':'standing waves',
 'Q = m c ΔT':'mc delta t q=mcdt',
 'Q = m L':'heat of fusion heat of vaporization phase change melting boiling',
 'Q/t = k A ΔT/d':'thermal conductivity heat conduction',
 'P V^{γ} = constant':'pv gamma pvgamma pv^gamma',
 'ΔU = Q − W':'delta u',
 'R = ρL/A':'resistance of a wire',
 'F = q v B sin(θ)':'qvb',
 'F = I L B sin(θ)':'ilb',
 'B = μ_{0} I/(2πr)':'biot savart',
 'ε = −N dΦ/dt':'lenz lenzs law',
 '∇ · E = ρ/ε_{0}':'divergence of e',
 '∇ · B = 0':'divergence of b',
 '∇ × E = −∂B/∂t':'curl e',
 '∇ × B = μ_{0} J + μ_{0} ε_{0} ∂E/∂t':'curl b',
 'E = m c^{2}':'e=mc^2 emc2',
 'E = h f':'planck relation',
 'λ = h/p':'debroglie matter wave',
 'KE = h f − φ':'work function threshold frequency',
 'V = I R':'v=ir',
 'P = V I = I^{2} R = V^{2}/R':'power dissipated dissipation',
 'R_{1}R_{2}/(R_{1} + R_{2})':'series resistance equivalent resistance',
 'x(t) = x(∞) + (x(0) − x(∞)) e^{−t/τ}':'rc circuit charging discharging transient response',
 'ω = 2πf':'omega 2 pi f',
 'Y = 1/Z = G + jB':'conductance susceptance',
 'Z_{in} = (N_{1}/N_{2})^{2} Z_{L}':'referred impedance',
 'ω_{0} = 1/√(LC)':'lc circuit tank circuit',
 '|H|_{dB} = 20 log_{10}|H|':'bode plot',
 '−3 dB and ±45°':'3db half power point',
 'V_{out} = −R_{f} (V_{1}/R_{1} + V_{2}/R_{2})':'summer adder',
 'V_{out} = (R_{2}/R_{1})(V_{2} − V_{1})':'differential amplifier subtractor',
 'I = I_{S} (e^{V/(nV_{T})} − 1)':'diode current',
 '¬(A · B) = ¬A + ¬B':'demorgans de morgans',
 '¬(A + B) = ¬A · ¬B':'demorgans',
 'LSB = V_{FS}/2^{n}':'adc resolution step size',
 'T = G/(1 + GH)':'negative feedback g/(1+gh)',
 'q″ = Q̇/A':'double prime',
 'Q̇ = h A (T − T_{∞})':'delta t',
 'Q̇ = ε σ A (T_{1}^{4} − T_{2}^{4})':'radiation exchange radiative heat transfer',
 'R = 1/(h A)':'1/ha',
 '1/(U A) = Σ R':'ua',
 'λ_{max} = b/T':'wien displacement',
 'σ_{1,2} = (σ_{x} + σ_{y})/2 ± √(((σ_{x} − σ_{y})/2)^{2} + τ_{xy}^{2})':'sigma 1 sigma1',
 'tan(2θ_{p}) = 2τ_{xy}/(σ_{x} − σ_{y})':'theta p',
 'n = σ_{Y}/σ_{v}':'fos',
 'δ_{T} = αΔT L':'thermal deformation thermal elongation',
 'Σ F = 0':'sum of forces',
 'v_{e} = √(2 G M/r)':'escape speed',
 'd sin(θ) = m λ':'diffraction grating',
 'V_{1}/V_{2} = N_{1}/N_{2}':'transformer',
 'I_{E} = I_{B} + I_{C}':'emitter current',
 'I_{D} = (1/2) μ_{n} C_{ox} (W/L) V_{ov}^{2}':'square law',
 'dB = 20 log_{10}(V_{2}/V_{1})':'voltage gain',
 'v_{n} = √(4 k T R B)':'thermal noise voltage',
 'δ = √(2/(ωμσ))':'skin effect',
 'ε_{ij} = (1/2)(∂u_{i}/∂x_{j} + ∂u_{j}/∂x_{i})':'infinitesimal strain',
 'p = −σ_{kk}/3':'mean stress hydrostatic stress',
 'σ = E ε':'sigma e epsilon',
 'ν = −ε_{lat}/ε_{axial}':'poissons',
 'a · b = a_{i} b_{i}':'einstein notation',
 'σ′_{ij} = Q_{ik} Q_{jl} σ_{kl}':'tensor transformation',
 'det(σ − λI) = 0':'eigenvalues',
 'I = π(d_{o}^{4} − d_{i}^{4})/64':'inertia',
 'wL^{4}/(8EI)':'distributed load',
 'h_{m} = K v^{2}/(2g)':'loss coefficient',
 'N = n N_{A}':'number of molecules atoms',
 'K_{a} K_{b} = K_{w} = 10^{−14}':'kb',
 't ≈ 10^{10} yr (M/M_{sun})^{−2.5}':'stellar lifetime',
 'ΔL = α L ΔT':'coefficient of thermal expansion',
 'σ_{v} = √(((σ_{1} − σ_{2})^{2} + (σ_{2} − σ_{3})^{2} + (σ_{3} − σ_{1})^{2})/2)':'vonmises',
 'Q̇ − Ẇ = ṁ (h_{2} − h_{1} + (v_{2}^{2} − v_{1}^{2})/2 + g(z_{2} − z_{1}))':'sfee',
 'σ = pr/(2t)':'spherical pressure vessel',
 'EI d^{2}v/dx^{2} = M':'elastic curve',
 'I = πd^{4}/64':'circle inertia',
 'F = p_{c} A':'submerged surface',
 'v = √(2gh)':'tank draining efflux',
 'h_{f} = f (L/D) v^{2}/(2g)':'major losses',
 'dB/dt = d/dt ∫_{CV} ρb dV + ∫_{CS} ρb (v · n) dA':'rtt',
 'n = m/M':'grams to moles',
 'c_{1} V_{1} = c_{2} V_{2}':'c1v1',
 'P_{1}/T_{1} = P_{2}/T_{2}':'gay lussac',
 '(P + a n^{2}/V^{2})(V − n b) = n R T':'vanderwaals',
 'q = m c ΔT':'mcat mc delta t q=mcdt',
 'ΔH° = Σ ΔH°_{f}(products) − Σ ΔH°_{f}(reactants)':'delta h enthalpy change',
 'ΔS° = Σ S°(products) − Σ S°(reactants)':'delta s',
 'ΔG = ΔH − T ΔS':'delta g',
 'ΔG° = −R T ln(K)':'delta g k',
 'ΔG = ΔG° + R T ln(Q)':'delta g nonstandard',
 'ΔG = −n F E':'delta g nfe',
 'Q < K':'q vs k',
 '[H^{+}] ≈ √(K_{a} c)':'ice table',
 'm = I t M/(n F)':'electroplating',
 'FC = V − N − B/2':'lewis structure',
 'N = N_{0} e^{rt}':'exponential growth equation population',
 'dN/dt = r N − a N P':'prey equation',
 'dP/dt = b N P − m P':'predator equation',
 'H = −Σ p_{i} ln(p_{i})':'biodiversity',
 'D = 1 − Σ p_{i}^{2}':'biodiversity',
 'C_{6}H_{12}O_{6} + 6O_{2} → 6CO_{2} + 6H_{2}O':'cellular respiration equation glucose',
 '6CO_{2} + 6H_{2}O → C_{6}H_{12}O_{6} + 6O_{2}':'photosynthesis equation',
 'Q = πR^{4}ΔP/(8ηL)':'blood flow vessel',
 'm_{1} − m_{2} = −2.5 log_{10}(F_{1}/F_{2})':'magnitude scale magnitude difference',
 '1 + z = λ_{obs}/λ_{emit}':'cosmological redshift expansion',
 'P(A^{c}) = 1 − P(A)':'at least one probability of none not happening',
}.items():
    TAGS[k]=(TAGS.get(k,'')+' '+v).strip()
for sh in native:
    for sec in sh['sections']:
        for g in sec['groups']:
            for it in g['items']:
                extra=TAGS.get(it.get('show'))
                if extra: it['tags']=(it.get('tags','')+' '+extra).strip()
native.extend(named_sheets(native))
out=json.dumps(native,ensure_ascii=False,indent=1)
open(sys.argv[1],'w',encoding='utf-8').write(out+'\n')
# the web's four sheets keep their data as it is (the golden suite holds it): their Typst is a table beside it
import os
here=os.path.dirname(os.path.abspath(sys.argv[1]))
web=json.load(open(os.path.join(os.path.dirname(os.path.abspath(__file__)),'..','core','src','app','identities.json'),encoding='utf-8'))
cells={}
for sh in web:
    table=cells.setdefault(sh['id'],{})
    units=sh['id'] in('conversions','constants')
    for sec in sh['sections']:
        for gr in sec['groups']:
            for it in gr['items']:
                for t in it.get('lead',[]):
                    table.setdefault(t,'"'+t+'"' if sh['id']=='conversions' else body(t))
                table.setdefault(it['show'],unit_body(it['show']) if units else body(it['show']))
# a conversion's number and unit each copy alone: 1 in = 2.54 cm
NUMBER=r'(?:[\d.]+ × 10\^\{[−\d]+\}|10\^\{[−\d]+\}|[\d.]+)'
def number_copy(n):
    m=re.fullmatch(r'([\d.]+) × 10\^\{([−\d]+)\}',n)
    if m: return m.group(1)+'e'+m.group(2).replace('−','-')
    m=re.fullmatch(r'10\^\{([−\d]+)\}',n)
    return '1e'+m.group(1).replace('−','-') if m else n
parts={}
for sh in web:
    if sh['id']!='conversions': continue
    for sec in sh['sections']:
        for gr in sec['groups']:
            for it in gr['items']:
                m=re.fullmatch('('+NUMBER+r') ?(.*?) = ('+NUMBER+r') ?(.*)',it['show'])
                if not m or it.get('lead'): continue
                row=[]
                for text,copy in [(m.group(1),number_copy(m.group(1))),(m.group(2),m.group(2) and plain(m.group(2))),('=',''),(m.group(3),number_copy(m.group(3))),(m.group(4),m.group(4) and plain(m.group(4)))]:
                    if text: row.append({'typst':unit_body(text),'copy':copy})
                parts[it['show']]=row
cells['parts']=parts
open(os.path.join(here,'native_typst_cells.json'),'w',encoding='utf-8').write(json.dumps(cells,ensure_ascii=False,indent=0)+'\n')
print(len(native),'sheets',sum(len(g['items']) for s in native for x in s['sections'] for g in x['groups']),'rows')
