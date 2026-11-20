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
  G('Analysis',
    I('a_{k} = (1/T) ∫_{T} x(t) e^{−jkω_{o}t} dt',note='ωₒ = 2π/T',lead=['CT']),
    I('a_{k} = (1/N) Σ_{n=<N>} x[n] e^{−jkω_{o}n}',note='ωₒ = 2π/N',lead=['DT'])),
  G('Synthesis',
    I('x(t) = Σ_{k=−∞}^{∞} a_{k} e^{jkω_{o}t}',note='ωₒ = 2π/T',lead=['CT']),
    I('x[n] = Σ_{k=<N>} a_{k} e^{jkω_{o}n}',note='ωₒ = 2π/N',lead=['DT'])),
  wide=True,leads=[44]),
 S('Trig form (real x)',
  G('Series',I('x(t) = A_{0} + Σ_{k=1}^{∞} [A_{k} cos(kω_{o}t) + B_{k} sin(kω_{o}t)]')),
  G('Coefficients',
    I('A_{0} = (1/T) ∫_{T} x(t) dt',note='the average'),
    I('A_{k} = (2/T) ∫_{T} x(t) cos(kω_{o}t) dt'),
    I('B_{k} = (2/T) ∫_{T} x(t) sin(kω_{o}t) dt'),
    I('a_{k} = (A_{k} − jB_{k})/2',note='k > 0'),
    I('a_{−k} = a_{k}^{*}',note='x real')),
  wide=True),
 S('Properties (CT)',
  G('A signal and its coefficients',
    H('Signal','Coefficient'),
    C('x(t − t_{0})','a_{k} e^{−jkω_{o}t_{0}}'),
    C('x(−t)','a_{−k}'),
    C('dx/dt','jkω_{o} a_{k}'),
    C('x(t) y(t)','Σ_{l} a_{l} b_{k−l}'),
    C('(1/T) ∫_{T} |x(t)|^{2} dt','Σ_{k} |a_{k}|^{2}',note='Parseval')),
  wide=True,leads=[180]))

euler=SH('euler',"Euler's Formulas","euler's formulas and related identities",
 S("Euler's formulas",
  G('Exponentials',
    I('e^{jθ} = cos(θ) + j sin(θ)'),
    I('e^{jθ} + e^{−jθ} = 2 cos(θ)'),
    I('e^{jθ} − e^{−jθ} = 2j sin(θ)')),
  G('Trig from exponentials',
    I('cos(θ) = (1/2) e^{jθ} + (1/2) e^{−jθ}'),
    I('sin(θ) = (1/2j) e^{jθ} − (1/2j) e^{−jθ}'),
    I('(cos θ + j sin θ)^{n} = cos(nθ) + j sin(nθ)',note='de Moivre'))),
 S('Useful trig identities',
  G('Squares',
    I('cos^{2}(A) = 1/2 + (1/2) cos(2A)',copy='cos(A)^2 = 1/2 + (1/2) cos(2A)'),
    I('sin^{2}(A) = 1/2 − (1/2) cos(2A)',copy='sin(A)^2 = 1/2 - (1/2) cos(2A)')),
  G('Products',
    I('cos(A) cos(B) = (1/2)[cos(A − B) + cos(A + B)]'),
    I('sin(A) sin(B) = (1/2)[cos(A − B) − cos(A + B)]'),
    I('sin(A) cos(B) = (1/2)[sin(A + B) + sin(A − B)]'),
    I('cos(A) sin(B) = (1/2)[sin(A + B) − sin(A − B)]'))))

complexn=SH('complex numbers','Complex Numbers','special complex numbers in polar form',
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
    I('1/j = −j'),
    I('j = i',note='either works in Q Calc'))),
 S('Rectangular and polar',
  G('Converting',
    I('z = x + jy = r e^{jθ}'),
    I('r = |z| = √(x^{2} + y^{2})'),
    I('θ = atan2(y, x)'),
    I('x = r cos(θ)'),
    I('y = r sin(θ)')),
  G('Arithmetic',
    I('z_{1} z_{2} = r_{1} r_{2} e^{j(θ_{1} + θ_{2})}'),
    I('z_{1} / z_{2} = (r_{1}/r_{2}) e^{j(θ_{1} − θ_{2})}'),
    I('z^{*} = x − jy = r e^{−jθ}',note='conjugate'),
    I('z z^{*} = |z|^{2}'),
    I('Re z = (z + z^{*})/2'),
    I('Im z = (z − z^{*})/2j'))))

ct=SH('ct transform table','CT Fourier Transform Table','ct fourier transform table',
 S('Definition',
  G('The pair',
    I('X(jω) = ∫_{−∞}^{∞} x(t) e^{−jωt} dt',lead=['Analysis']),
    I('x(t) = (1/2π) ∫_{−∞}^{∞} X(jω) e^{jωt} dω',lead=['Synthesis'])),
  wide=True,leads=[80]),
 S('Pairs',
  G('Common signals',
    H('Time','Frequency'),
    P('e^{−at} u(t), a > 0','1/(a + jω)'),
    P('t e^{−at} u(t), a > 0','1/(a + jω)^{2}'),
    P('δ(t)','1'),
    P('δ(t − t_{0})','e^{−jωt_{0}}'),
    P('1','2πδ(ω)'),
    P('u(t)','1/(jω) + πδ(ω)'),
    P('e^{jω_{o}t}','2πδ(ω − ω_{o})'),
    P('cos(ω_{o}t)','πδ(ω − ω_{o}) + πδ(ω + ω_{o})'),
    P('sin(ω_{o}t)','(π/j)[δ(ω − ω_{o}) − δ(ω + ω_{o})]'),
    P('sin(Wt)/(πt)','1 for |ω| < W, 0 else'),
    P('1 for |t| < T_{1}, 0 else','2 sin(ωT_{1})/ω'),
    P('e^{−a|t|}, a > 0','2a/(a^{2} + ω^{2})'),
    P('e^{−t^{2}/(2σ^{2})}','σ√(2π) e^{−σ^{2}ω^{2}/2}'),
    P('Periodic p(t) with F.S. {ω_{o}, a_{k}}','P(jω) = Σ_{k=−∞}^{∞} 2πa_{k} δ(ω − kω_{o})'),
    P('Σ_{n=−∞}^{∞} δ(t − nT)','(2π/T) Σ_{k=−∞}^{∞} δ(ω − 2πk/T)')),
  wide=True,leads=[270]),
 S('Properties',
  G('If x(t) has transform X(jω)',
    H('Time','Frequency'),
    P('d^{n}x(t)/dt^{n}','(jω)^{n} X(jω)'),
    P('y(t) = x(t) ∗ h(t)','Y(jω) = X(jω) H(jω)'),
    P('y(t) = x(t) p(t)','Y(jω) = (1/2π) X(jω) ∗ P(jω)'),
    P('x(t − t_{0})','e^{−jωt_{0}} X(jω)'),
    P('e^{jω_{o}t} x(t)','X(j(ω − ω_{o}))'),
    P('x(at)','(1/|a|) X(jω/a)'),
    P('x(−t)','X(−jω)'),
    P('t x(t)','j dX(jω)/dω'),
    P('∫_{−∞}^{t} x(τ) dτ','X(jω)/(jω) + πX(0)δ(ω)'),
    P('X(jt)','2πx(−ω)',note='duality'),
    P('∫ |x(t)|^{2} dt','(1/2π) ∫ |X(jω)|^{2} dω',note='Parseval')),
  wide=True,leads=[270]))

dt=SH('dt transform table','DT Fourier Transform Table','dt fourier transform table',
 S('Definition',
  G('The pair',
    I('X(e^{jω}) = Σ_{n=−∞}^{∞} x[n] e^{−jωn}',lead=['Analysis']),
    I('x[n] = (1/2π) ∫_{2π} X(e^{jω}) e^{jωn} dω',lead=['Synthesis'])),
  wide=True,leads=[80]),
 S('Pairs',
  G('Common signals (every X repeats each 2π; impulses are shown for one period)',
    H('Sample','Frequency'),
    P('a^{n} u[n], |a| < 1','1/(1 − ae^{−jω})'),
    P('(n + 1) a^{n} u[n], |a| < 1','1/(1 − ae^{−jω})^{2}'),
    P('δ[n]','1'),
    P('δ[n − n_{o}]','e^{−jωn_{o}}'),
    P('1','2πδ(ω)'),
    P('u[n]','1/(1 − e^{−jω}) + πδ(ω)'),
    P('e^{jω_{o}n}','2πδ(ω − ω_{o})'),
    P('cos(ω_{o}n)','πδ(ω − ω_{o}) + πδ(ω + ω_{o})'),
    P('sin(ω_{o}n)','(π/j)[δ(ω − ω_{o}) − δ(ω + ω_{o})]'),
    P('sin(Wn)/(πn), 0 < W < π','1 for |ω| < W, 0 else'),
    P('1 for |n| ≤ N_{1}, 0 else','sin(ω(N_{1} + 1/2))/sin(ω/2)')),
  wide=True,leads=[270]),
 S('Properties',
  G('If x[n] has transform X(e^jω)',
    H('Sample','Frequency'),
    P('x[n − n_{o}]','e^{−jωn_{o}} X(e^{jω})'),
    P('y[n] = x[n] ∗ h[n]','Y(e^{jω}) = X(e^{jω}) H(e^{jω})'),
    P('e^{jω_{o}n} x[n]','X(e^{j(ω − ω_{o})})'),
    P('x[−n]','X(e^{−jω})'),
    P('n x[n]','j dX(e^{jω})/dω'),
    P('x[n] − x[n − 1]','(1 − e^{−jω}) X(e^{jω})'),
    P('x[n] y[n]','(1/2π) ∫_{2π} X(e^{jθ}) Y(e^{j(ω − θ)}) dθ'),
    P('Σ_{n} |x[n]|^{2}','(1/2π) ∫_{2π} |X(e^{jω})|^{2} dω',note='Parseval')),
  wide=True,leads=[270]),
 S('DFT',
  G('N-point',
    I('X[k] = Σ_{n=0}^{N−1} x[n] e^{−j2πkn/N}',lead=['Analysis']),
    I('x[n] = (1/N) Σ_{k=0}^{N−1} X[k] e^{j2πkn/N}',lead=['Synthesis'])),
  wide=True,leads=[80]))

laplace=SH('laplace','Laplace Transform Table','laplace transform table',
 S('Definition',
  G('One-sided',I('F(s) = ∫_{0}^{∞} f(t) e^{−st} dt')),wide=True),
 S('Pairs',
  G('For t ≥ 0',
    H('f(t)','F(s)'),
    P('δ(t)','1'),
    P('u(t)','1/s'),
    P('t','1/s^{2}'),
    P('t^{n}','n!/s^{n+1}'),
    P('e^{−at}','1/(s + a)'),
    P('t e^{−at}','1/(s + a)^{2}'),
    P('t^{n} e^{−at}','n!/(s + a)^{n+1}'),
    P('sin(ωt)','ω/(s^{2} + ω^{2})'),
    P('cos(ωt)','s/(s^{2} + ω^{2})'),
    P('e^{−at} sin(ωt)','ω/((s + a)^{2} + ω^{2})'),
    P('e^{−at} cos(ωt)','(s + a)/((s + a)^{2} + ω^{2})'),
    P('sinh(at)','a/(s^{2} − a^{2})'),
    P('cosh(at)','s/(s^{2} − a^{2})')),
  wide=True,leads=[270]),
 S('Properties',
  G('If f(t) has transform F(s)',
    H('f(t)','F(s)'),
    P('f(t − a) u(t − a), a ≥ 0','e^{−as} F(s)'),
    P('e^{−at} f(t)','F(s + a)'),
    P('f(at), a > 0','(1/a) F(s/a)'),
    P('df/dt','s F(s) − f(0)'),
    P('d^{2}f/dt^{2}','s^{2} F(s) − s f(0) − f′(0)'),
    P('∫_{0}^{t} f(τ) dτ','F(s)/s'),
    P('t f(t)','−dF/ds'),
    P('f(t) ∗ g(t)','F(s) G(s)')),
  G('Limits',
    I('f(0⁺) = lim_{s→∞} s F(s)',note='initial value'),
    I('f(∞) = lim_{s→0} s F(s)',note='final value, if the limit exists')),
  wide=True,leads=[270]))

ztr=SH('z transform','Z Transform Table','z transform table',
 S('Definition',
  G('Two-sided',I('X(z) = Σ_{n=−∞}^{∞} x[n] z^{−n}')),wide=True),
 S('Pairs',
  G('With the region of convergence',
    H('x[n]','X(z)'),
    P('δ[n]','1',note='all z'),
    P('u[n]','1/(1 − z^{−1})',note='|z| > 1'),
    P('a^{n} u[n]','1/(1 − az^{−1})',note='|z| > |a|'),
    P('−a^{n} u[−n − 1]','1/(1 − az^{−1})',note='|z| < |a|'),
    P('n a^{n} u[n]','az^{−1}/(1 − az^{−1})^{2}',note='|z| > |a|'),
    P('cos(ω_{o}n) u[n]','(1 − cos(ω_{o}) z^{−1})/(1 − 2cos(ω_{o}) z^{−1} + z^{−2})',note='|z| > 1'),
    P('sin(ω_{o}n) u[n]','sin(ω_{o}) z^{−1}/(1 − 2cos(ω_{o}) z^{−1} + z^{−2})',note='|z| > 1')),
  wide=True,leads=[190]),
 S('Properties',
  G('If x[n] has transform X(z)',
    H('x[n]','X(z)'),
    P('x[n − n_{o}]','z^{−n_{o}} X(z)'),
    P('a^{n} x[n]','X(z/a)'),
    P('n x[n]','−z dX/dz'),
    P('x[−n]','X(1/z)'),
    P('x[n] ∗ h[n]','X(z) H(z)')),
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
  wide=True,leads=[80,80,80,80]))

geom=SH('geometry','Geometry Formulas','geometry formulas',
 S('Area and perimeter',
  G('Round',
    I('A = πr^{2}',note='circle'),
    I('C = 2πr',note='circumference'),
    I('A = (1/2) r^{2} θ',note='sector, θ in radians'),
    I('s = rθ',note='arc, θ in radians'),
    I('A = πab',note='ellipse')),
  G('Straight',
    I('A = (1/2) b h',note='triangle'),
    I('A = √(s(s − a)(s − b)(s − c))',note='Heron, s = half perimeter'),
    I('A = (1/2) a b sin(C)',note='triangle'),
    I('A = (1/2)(a + b) h',note='trapezoid'),
    I('A = b h',note='parallelogram'))),
 S('Surface and volume',
  G('Volume',
    I('V = (4/3) πr^{3}',note='sphere'),
    I('V = πr^{2} h',note='cylinder'),
    I('V = (1/3) πr^{2} h',note='cone'),
    I('V = (1/3) B h',note='pyramid, base area B'),
    I('V = B h',note='prism')),
  G('Surface',
    I('S = 4πr^{2}',note='sphere'),
    I('S = 2πr^{2} + 2πrh',note='cylinder'),
    I('S = πr^{2} + πr√(r^{2} + h^{2})',note='cone'))),
 S('Triangles and coordinates',
  G('Triangles',
    I('a^{2} + b^{2} = c^{2}',note='Pythagoras'),
    I('c^{2} = a^{2} + b^{2} − 2ab cos(C)',note='law of cosines'),
    I('a/sin(A) = b/sin(B) = c/sin(C)',note='law of sines')),
  G('Coordinates',
    I('d = √((x_{2} − x_{1})^{2} + (y_{2} − y_{1})^{2})',note='distance'),
    I('((x_{1} + x_{2})/2, (y_{1} + y_{2})/2)',note='midpoint'),
    I('m = (y_{2} − y_{1})/(x_{2} − x_{1})',note='slope'))))

stats=SH('statistics','Statistics and Probability','statistics and probability',
 S('Describing data',
  G('Sample',
    I('x̄ = (1/n) Σ x_{i}',note='mean'),
    I('s^{2} = Σ (x_{i} − x̄)^{2} / (n − 1)',note='variance'),
    I('s = √(s^{2})',note='standard deviation'),
    I('z = (x − μ)/σ',note='z-score')),
  G('Expectation',
    I('E[X] = Σ x p(x)'),
    I('Var(X) = E[X^{2}] − E[X]^{2}'),
    I('E[aX + b] = a E[X] + b'),
    I('Var(aX + b) = a^{2} Var(X)'))),
 S('Probability',
  G('Rules',
    I('P(A ∪ B) = P(A) + P(B) − P(A ∩ B)'),
    I('P(A | B) = P(A ∩ B)/P(B)'),
    I('P(A | B) = P(B | A) P(A)/P(B)',note='Bayes')),
  G('Counting',
    I('nPr = n!/(n − r)!',note='ordered'),
    I('nCr = n!/(r!(n − r)!)',note='unordered'))),
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
 S('Inference',
  G('Confidence',
    I('x̄ ± z σ/√n',note='interval for a mean'),
    I('z = 1.645',note='90%',copy='1.645'),
    I('z = 1.960',note='95%',copy='1.96'),
    I('z = 2.576',note='99%',copy='2.576')),
  G('The normal curve',
    I('within 1σ: 68.27%',copy='68.27%'),
    I('within 2σ: 95.45%',copy='95.45%'),
    I('within 3σ: 99.73%',copy='99.73%'))))

linalg=SH('linear algebra','Linear Algebra','linear algebra',
 S('Matrices',
  G('2 × 2, A = [a b; c d]',
    I('det A = ad − bc'),
    I('A^{−1} = (1/det A) [d −b; −c a]')),
  G('3 × 3, A = [a b c; d e f; g h i]',
    I('det A = a(ei − fh) − b(di − fg) + c(dh − eg)')),
  G('Rules',
    I('(AB)^{T} = B^{T} A^{T}'),
    I('(AB)^{−1} = B^{−1} A^{−1}'),
    I('det(AB) = det(A) det(B)'),
    I('det(A^{T}) = det(A)'),
    I('det(A^{−1}) = 1/det(A)'))),
 S('Vectors',
  G('Products',
    I('a · b = Σ a_{i} b_{i} = |a| |b| cos(θ)'),
    I('|a × b| = |a| |b| sin(θ)'),
    I('a × b = (a_{2}b_{3} − a_{3}b_{2}, a_{3}b_{1} − a_{1}b_{3}, a_{1}b_{2} − a_{2}b_{1})')),
  G('Length and projection',
    I('|a| = √(a · a)'),
    I('proj_{b} a = ((a · b)/(b · b)) b')),
  wide=True),
 S('Eigenvalues',
  G('Definitions',
    I('A v = λ v'),
    I('det(A − λI) = 0',note='characteristic equation')),
  G('Checks',
    I('tr(A) = Σ λ_{i}'),
    I('det(A) = Π λ_{i}'))))

physics=SH('physics','Physics Formulas','physics formulas',
 S('Mechanics',
  G('Constant acceleration',
    I('v = v_{0} + a t'),
    I('x = x_{0} + v_{0} t + (1/2) a t^{2}'),
    I('v^{2} = v_{0}^{2} + 2a Δx',copy='v^2 = v_0^2 + 2a Δx'),
    I('Δx = (1/2)(v_{0} + v) t')),
  G('Force and energy',
    I('F = m a'),
    I('W = F d cos(θ)',note='work'),
    I('KE = (1/2) m v^{2}'),
    I('PE = m g h'),
    I('P = dW/dt = F v',note='power'),
    I('p = m v',note='momentum')),
  G('Rotation',
    I('a_{c} = v^{2}/r',note='centripetal'),
    I('τ = r F sin(θ)',note='torque'),
    I('τ = I α'),
    I('L = I ω',note='angular momentum'),
    I('KE = (1/2) I ω^{2}')),
  G('Gravity and springs',
    I('F = G m_{1} m_{2}/r^{2}'),
    I('F = −k x',note='Hooke'),
    I('PE = (1/2) k x^{2}',note='spring'),
    I('T = 2π√(m/k)',note='mass on a spring'),
    I('T = 2π√(L/g)',note='pendulum'))),
 S('Waves, heat and modern',
  G('Waves',
    I('v = f λ'),
    I('T = 1/f'),
    I('n_{1} sin(θ_{1}) = n_{2} sin(θ_{2})',note='Snell')),
  G('Heat and gases',
    I('P V = n R T',note='ideal gas'),
    I('Q = m c ΔT'),
    I('ΔU = Q − W',note='first law')),
  G('Fields',
    I('F = k q_{1} q_{2}/r^{2}',note='Coulomb'),
    I('F = q(E + v × B)',note='Lorentz')),
  G('Modern',
    I('E = m c^{2}'),
    I('E = h f'),
    I('λ = h/p',note='de Broglie'))),
 S('Electricity and magnetism',
  G('Fields and potential',
    I('E = kq/r^{2}',note='point charge'),
    I('V = kq/r',note='point charge'),
    I('U = qV',note='potential energy'),
    I('E = −dV/dx'),
    I('C = ε_{0} A/d',note='parallel plates')),
  G('Current',
    I('R = ρL/A',note='resistivity'),
    I('I = n q A v_{d}',note='drift speed')),
  G('Magnetism',
    I('F = q v B sin(θ)',note='on a charge'),
    I('F = I L B sin(θ)',note='on a wire'),
    I('B = μ_{0} I/(2πr)',note='long wire'),
    I('B = μ_{0} n I',note='solenoid'),
    I('Φ = B A cos(θ)',note='flux'),
    I('ε = −N dΦ/dt',note='Faraday')),
  G('Maxwell',
    I('∇ · E = ρ/ε_{0}',note='Gauss'),
    I('∇ · B = 0'),
    I('∇ × E = −∂B/∂t',note='Faraday'),
    I('∇ × B = μ_{0} J + μ_{0} ε_{0} ∂E/∂t',note='Ampère'))),
 S('Thermodynamics',
  G('Laws and engines',
    I('ΔS = Q/T',note='reversible'),
    I('η = 1 − T_{c}/T_{h}',note='Carnot'),
    I('η = W/Q_{h}',note='any engine'),
    I('W = P ΔV',note='constant pressure'),
    I('W = n R T ln(V_{2}/V_{1})',note='isothermal')),
  G('Gases and heat flow',
    I('U = (3/2) n R T',note='monatomic'),
    I('v_{rms} = √(3RT/M)'),
    I('P V^{γ} = constant',note='adiabatic'),
    I('Q = m L',note='change of phase'),
    I('Q/t = k A ΔT/d',note='conduction'),
    I('P = e σ A T^{4}',note='radiation'))),
 S('Waves, sound and optics',
  G('Waves and sound',
    I('v = √(T/μ)',note='string'),
    I('f_{n} = n v/(2L)',note='string, both ends fixed'),
    I('f′ = f (v + v_{o})/(v − v_{s})',note='Doppler, approaching'),
    I('β = 10 log_{10}(I/I_{0})',note='decibels'),
    I('I = P/(4πr^{2})',note='intensity')),
  G('Optics',
    I('1/f = 1/d_{o} + 1/d_{i}',note='thin lens'),
    I('m = −d_{i}/d_{o}',note='magnification'),
    I('n = c/v',note='refractive index'),
    I('sin(θ_{c}) = n_{2}/n_{1}',note='critical angle'),
    I('d sin(θ) = m λ',note='double slit, bright'))),
 S('Relativity, quantum and nuclear',
  G('Relativity',
    I('γ = 1/√(1 − v^{2}/c^{2})'),
    I('t = γ t_{0}',note='time dilation'),
    I('L = L_{0}/γ',note='length contraction'),
    I('E^{2} = (pc)^{2} + (mc^{2})^{2}')),
  G('Quantum',
    I('KE = h f − φ',note='photoelectric'),
    I('Δx Δp ≥ ħ/2',note='uncertainty'),
    I('E_{n} = −13.6 eV/n^{2}',note='hydrogen'),
    I('iħ ∂ψ/∂t = H ψ',note='Schrödinger')),
  G('Nuclear',
    I('N = N_{0} e^{−λt}',note='decay'),
    I('t_{1/2} = ln(2)/λ',note='half-life'),
    I('E = Δm c^{2}',note='binding energy'))))

chemistry=SH('chemistry','Chemistry Formulas','chemistry formulas',
 S('Amounts and solutions',
  G('Moles',
    I('n = m/M',note='M is molar mass'),
    I('N = n N_{A}',note='particles'),
    I('percent yield = 100 × actual/theoretical')),
  G('Concentration',
    I('c = n/V',note='molarity'),
    I('b = n/m_{solvent}',note='molality'),
    I('c_{1} V_{1} = c_{2} V_{2}',note='dilution'),
    I('A = ε l c',note='Beer-Lambert'))),
 S('Gas laws',
  G('Ideal',
    I('P V = n R T'),
    I('P_{1} V_{1}/T_{1} = P_{2} V_{2}/T_{2}',note='combined'),
    I('P_{tot} = Σ P_{i}',note='Dalton'),
    I('P_{i} = x_{i} P_{tot}',note='mole fraction')),
  G('Real, and moving',
    I('(P + a n^{2}/V^{2})(V − n b) = n R T',note='van der Waals'),
    I('r_{1}/r_{2} = √(M_{2}/M_{1})',note='Graham'),
    I('v_{rms} = √(3RT/M)'))),
 S('Thermochemistry',
  G('Heat and free energy',
    I('q = m c ΔT'),
    I('ΔH° = Σ ΔH°_{f}(products) − Σ ΔH°_{f}(reactants)',note='Hess'),
    I('ΔG = ΔH − T ΔS',note='negative is spontaneous'),
    I('ΔG° = −R T ln(K)'),
    I('ΔG = ΔG° + R T ln(Q)'),
    I('ln(P_{2}/P_{1}) = −(ΔH_{vap}/R)(1/T_{2} − 1/T_{1})',note='Clausius-Clapeyron')),
  wide=True),
 S('Equilibrium, acids and bases',
  G('Equilibrium',
    I('K = ([C]^{c} [D]^{d})/([A]^{a} [B]^{b})',note='aA + bB ⇌ cC + dD'),
    I('K_{p} = K_{c} (RT)^{Δn}'),
    I('Q < K',note='runs forward'),
    I('K_{sp} = [A]^{a} [B]^{b}',note='solubility')),
  G('Acids and bases',
    I('pH = −log_{10}[H^{+}]'),
    I('pOH = −log_{10}[OH^{−}]'),
    I('pH + pOH = 14',note='25 °C'),
    I('K_{a} K_{b} = K_{w} = 10^{−14}',note='25 °C'),
    I('pH = pK_{a} + log_{10}([A^{−}]/[HA])',note='Henderson-Hasselbalch'))),
 S('Kinetics',
  G('Rate laws',
    H('Order','Integrated','Half life'),
    L('0','[A] = [A]_{0} − k t','[A]_{0}/(2k)'),
    L('1','ln[A] = ln[A]_{0} − k t','ln(2)/k'),
    L('2','1/[A] = 1/[A]_{0} + k t','1/(k[A]_{0})')),
  G('Rate and temperature',
    I('rate = k [A]^{m} [B]^{n}'),
    I('k = A e^{−E_{a}/(RT)}',note='Arrhenius'),
    I('ln(k_{2}/k_{1}) = −(E_{a}/R)(1/T_{2} − 1/T_{1})')),
  wide=True,leads=[60,240]),
 S('Electrochemistry',
  G('Cells',
    I('E°_{cell} = E°_{cathode} − E°_{anode}'),
    I('ΔG = −n F E'),
    I('E = E° − (RT/(nF)) ln(Q)',note='Nernst'),
    I('E = E° − (0.0592/n) log_{10}(Q)',note='Nernst, 25 °C'),
    I('m = I t M/(n F)',note='Faraday, mass plated')),
  wide=True),
 S('Colligative properties and light',
  G('Solutions',
    I('ΔT_{b} = i K_{b} b',note='boiling point rises'),
    I('ΔT_{f} = i K_{f} b',note='freezing point falls'),
    I('π = i c R T',note='osmotic pressure'),
    I('P = x P°',note='Raoult')),
  G('Light and atoms',
    I('E = h c/λ',note='photon'),
    I('1/λ = R_{H} (1/n_{1}^{2} − 1/n_{2}^{2})',note='Rydberg'))))

biology=SH('biology','Biology Formulas','biology formulas',
 S('Populations and ecology',
  G('Growth',
    I('dN/dt = r N',note='exponential'),
    I('N = N_{0} e^{rt}'),
    I('dN/dt = r N (1 − N/K)',note='logistic'),
    I('t_{d} = ln(2)/r',note='doubling time')),
  G('Diversity',
    I('H = −Σ p_{i} ln(p_{i})',note='Shannon'),
    I('D = 1 − Σ p_{i}^{2}',note='Simpson'))),
 S('Genetics',
  G('Allele frequencies',
    I('p + q = 1'),
    I('p^{2} + 2pq + q^{2} = 1',note='Hardy-Weinberg')),
  G('Tests and traits',
    I('χ^{2} = Σ (O − E)^{2}/E',note='chi-square'),
    I('h^{2} = V_{A}/V_{P}',note='heritability'),
    I('R = h^{2} S',note='breeder'),
    I('RF = recombinants/total',note='recombination frequency'))),
 S('Enzymes',
  G('Kinetics',
    I('v = V_{max} [S]/(K_{m} + [S])',note='Michaelis-Menten'),
    I('1/v = (K_{m}/V_{max})(1/[S]) + 1/V_{max}',note='Lineweaver-Burk'),
    I('k_{cat} = V_{max}/[E]',note='turnover number'),
    I('K_{app} = K_{m} (1 + [I]/K_{i})',note='competitive inhibitor'),
    I('θ = [L]^{n}/(K_{d} + [L]^{n})',note='Hill')),
  wide=True),
 S('Membranes and transport',
  G('Potentials',
    I('E = (RT/(zF)) ln([X]_{out}/[X]_{in})',note='Nernst'),
    I('E = (61.5 mV/z) log_{10}([X]_{out}/[X]_{in})',note='Nernst, 37 °C')),
  G('Moving across',
    I('J = −D dC/dx',note='Fick'),
    I('Ψ = Ψ_{s} + Ψ_{p}',note='water potential'),
    I('Ψ_{s} = −i C R T',note='solute potential')),
  wide=True),
 S('Metabolism',
  G('Overall reactions',
    I('6CO_{2} + 6H_{2}O → C_{6}H_{12}O_{6} + 6O_{2}',note='photosynthesis'),
    I('C_{6}H_{12}O_{6} + 6O_{2} → 6CO_{2} + 6H_{2}O',note='respiration')),
  G('Energy and temperature',
    I('ΔG°′ = −30.5 kJ/mol',note='ATP hydrolysis'),
    I('Q_{10} = (R_{2}/R_{1})^{10/(T_{2} − T_{1})}',note='rate per 10 °C')),
  wide=True),
 S('Physiology and pharmacology',
  G('Body',
    I('CO = HR × SV',note='cardiac output'),
    I('MAP = CO × TPR',note='mean arterial pressure'),
    I('Q = πR^{4}ΔP/(8ηL)',note='Poiseuille'),
    I('BMI = m/h^{2}',note='kg and metres')),
  G('Drugs',
    I('C = C_{0} e^{−kt}',note='first-order'),
    I('t_{1/2} = ln(2)/k',note='half-life'),
    I('V_{d} = dose/C_{0}',note='volume of distribution'))),
 S('Lab',
  G('Solutions and counts',
    I('c_{1} V_{1} = c_{2} V_{2}',note='dilution'),
    I('A = ε l c',note='Beer-Lambert'),
    I('N = N_{0} 2^{n}',note='PCR, n cycles'),
    I('N = N_{0} 2^{t/g}',note='g is generation time'))))

circuits=SH('circuits','Electrical Engineering','electrical engineering',
 S('Basics',
  G('Laws',
    I('V = I R',note='Ohm'),
    I('P = V I = I^{2} R = V^{2}/R',note='power'),
    I('Σ I = 0',note='KCL, at a node'),
    I('Σ V = 0',note='KVL, around a loop')),
  G('Dividers',
    I('V_{out} = V_{in} R_{2}/(R_{1} + R_{2})',note='voltage'),
    I('I_{1} = I R_{2}/(R_{1} + R_{2})',note='current')),
  G('Combining',
    H('Part','Series','Parallel'),
    L('R','R_{1} + R_{2}','R_{1}R_{2}/(R_{1} + R_{2})'),
    L('L','L_{1} + L_{2}','L_{1}L_{2}/(L_{1} + L_{2})'),
    L('C','C_{1}C_{2}/(C_{1} + C_{2})','C_{1} + C_{2}')),
  G('Networks',
    I('V_{out} = V_{s} (R_{2}/(R_{1} + R_{2}) − R_{4}/(R_{3} + R_{4}))',note='Wheatstone bridge'),
    I('R_{1} = R_{b} R_{c}/(R_{a} + R_{b} + R_{c})',note='delta to wye'),
    I('R_{a} = (R_{1}R_{2} + R_{2}R_{3} + R_{3}R_{1})/R_{1}',note='wye to delta')),
  G('Thévenin',
    I('V_{th} = V_{oc}'),
    I('R_{th} = V_{oc}/I_{sc}'),
    I('I_{N} = I_{sc}',note='Norton')),
  leads=[40,130]),
 S('Capacitors, inductors and AC',
  G('Storing energy',
    I('Q = C V'),
    I('i = C dv/dt'),
    I('v = L di/dt'),
    I('E = (1/2) C V^{2}'),
    I('E = (1/2) L I^{2}')),
  G('Impedance',
    I('Z_{R} = R'),
    I('Z_{L} = jωL'),
    I('Z_{C} = 1/(jωC)'),
    I('V_{rms} = V_{peak}/√2',note='sine')),
  G('First order',
    I('τ = R C'),
    I('τ = L/R'),
    I('x(t) = x(∞) + (x(0) − x(∞)) e^{−t/τ}'),
    I('f_{c} = 1/(2πRC)',note='cutoff')),
  G('Second order and gain',
    I('ω_{0} = 1/√(LC)',note='resonance'),
    I('Q = (1/R) √(L/C)',note='series RLC'),
    I('Q = R √(C/L)',note='parallel RLC'),
    I('dB = 20 log_{10}(V_{out}/V_{in})')),
  G('Ideal op amp',
    I('V_{out}/V_{in} = −R_{f}/R_{in}',note='inverting'),
    I('V_{out}/V_{in} = 1 + R_{f}/R_{g}',note='non-inverting'))),
 S('AC power and phasors',
  G('Phasors',
    I('v(t) = V_{m} cos(ωt + φ)'),
    I('V = V_{m} ∠φ',note='phasor'),
    I('ω = 2πf'),
    I('Z = R + jX',note='impedance'),
    I('Y = 1/Z = G + jB',note='admittance'),
    I('X_{L} = ωL',note='reactance'),
    I('|X_{C}| = 1/(ωC)',note='reactance')),
  G('Power',
    I('P = V_{rms} I_{rms} cos(θ)',note='real, W'),
    I('Q = V_{rms} I_{rms} sin(θ)',note='reactive, VAR'),
    I('S = V_{rms} I_{rms}^{*} = P + jQ',note='complex, VA'),
    I('PF = P/|S| = cos(θ)',note='power factor'),
    I('Z_{L} = Z_{th}^{*}',note='maximum power transfer'),
    I('P_{max} = V_{th}^{2}/(4R_{th})',note='V rms, matched load')),
  G('Three-phase',
    I('V_{L} = √3 V_{ph}',note='wye'),
    I('I_{L} = I_{ph}',note='wye'),
    I('V_{L} = V_{ph}',note='delta'),
    I('I_{L} = √3 I_{ph}',note='delta'),
    I('P = √3 V_{L} I_{L} cos(θ)',note='balanced')),
  G('Transformers (ideal)',
    I('V_{1}/V_{2} = N_{1}/N_{2}'),
    I('I_{1}/I_{2} = N_{2}/N_{1}'),
    I('Z_{in} = (N_{1}/N_{2})^{2} Z_{L}',note='seen from the primary'))),
 S('RLC and filters',
  G('RLC',
    I('ω_{0} = 1/√(LC)',note='resonance'),
    I('α = R/(2L)',note='series'),
    I('α = 1/(2RC)',note='parallel'),
    I('ζ = α/ω_{0}',note='damping ratio'),
    I('s = −α ± √(α^{2} − ω_{0}^{2})',note='natural roots'),
    I('ω_{d} = √(ω_{0}^{2} − α^{2})',note='underdamped'),
    I('BW = ω_{0}/Q',note='rad/s')),
  G('Second-order step response',
    I('H(s) = ω_{0}^{2}/(s^{2} + 2ζω_{0} s + ω_{0}^{2})'),
    I('%OS = 100 e^{−πζ/√(1 − ζ^{2})}',note='overshoot'),
    I('t_{p} = π/ω_{d}',note='peak time'),
    I('t_{s} ≈ 4/(ζω_{0})',note='settling, 2%'),
    I('t_{r} ≈ 1.8/ω_{0}',note='rise time, approximate')),
  G('First-order filters',
    I('H = 1/(1 + jωRC)',note='low-pass'),
    I('H = jωRC/(1 + jωRC)',note='high-pass'),
    I('f_{c} = 1/(2πRC)',note='RC cutoff'),
    I('f_{c} = R/(2πL)',note='RL cutoff')),
  G('Bode',
    I('|H|_{dB} = 20 log_{10}|H|'),
    I('−3 dB and ±45°',note='at a first-order corner',copy='-3 dB'),
    I('20 dB/decade',note='slope per pole or zero'))),
 S('Op amps',
  G('Ideal rules',
    I('V_{+} = V_{−}',note='with negative feedback'),
    I('I_{+} = I_{−} = 0')),
  G('Limits',
    I('GBW = A f',note='gain-bandwidth'),
    I('SR = 2πf V_{pk}',note='full-power slew rate'),
    I('CMRR = 20 log_{10}(A_{d}/A_{cm})')),
  G('Circuits',
    I('V_{out} = −R_{f} (V_{1}/R_{1} + V_{2}/R_{2})',note='summing'),
    I('V_{out} = (R_{2}/R_{1})(V_{2} − V_{1})',note='difference, matched'),
    I('V_{out} = −(1/(RC)) ∫ V_{in} dt',note='integrator'),
    I('V_{out} = −RC dV_{in}/dt',note='differentiator'),
    I('V_{out} = V_{in}',note='buffer')),
  wide=True),
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
    I('P = a C V^{2} f',note='CMOS switching, a activity')),
  wide=True),
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
    I('β = 2π/λ'),
    I('Γ = (Z_{L} − Z_{0})/(Z_{L} + Z_{0})',note='reflection'),
    I('VSWR = (1 + |Γ|)/(1 − |Γ|)'),
    I('Z_{in} = Z_{0} (Z_{L} + jZ_{0} tan(βl))/(Z_{0} + jZ_{L} tan(βl))',note='lossless')),
  G('Components and fields',
    I('C = εA/d',note='parallel plates'),
    I('L = μN^{2}A/l',note='solenoid'),
    I('δ = √(2/(ωμσ))',note='skin depth')),
  wide=True),
 S('Motors and power electronics',
  G('Machines',
    I('V = E + I_{a} R_{a}',note='DC motor'),
    I('E = k ω',note='back EMF'),
    I('τ = k I_{a}',note='torque'),
    I('P = τ ω'),
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
    I('e_{ss} = 1/(1 + K_{p})',note='step, type 0'),
    I('K_{p} = lim_{s→0} G(s)')),
  G('PID',
    I('u = K_{p} e + K_{i} ∫ e dt + K_{d} de/dt'),
    I('C(s) = K_{p} + K_{i}/s + K_{d} s'))))

taylor=SH('taylor series','Taylor Series','taylor series',
 S('General',
  G('About a',I('f(x) = Σ_{n=0}^{∞} f^{(n)}(a) (x − a)^{n}/n!')),wide=True),
 S('Maclaurin series',
  G('Common functions',
    I('e^{x} = Σ x^{n}/n! = 1 + x + x^{2}/2! + x^{3}/3! + …',note='all x'),
    I('sin x = Σ (−1)^{n} x^{2n+1}/(2n + 1)! = x − x^{3}/3! + x^{5}/5! − …',note='all x'),
    I('cos x = Σ (−1)^{n} x^{2n}/(2n)! = 1 − x^{2}/2! + x^{4}/4! − …',note='all x'),
    I('sinh x = x + x^{3}/3! + x^{5}/5! + …',note='all x'),
    I('cosh x = 1 + x^{2}/2! + x^{4}/4! + …',note='all x'),
    I('1/(1 − x) = Σ x^{n} = 1 + x + x^{2} + x^{3} + …',note='|x| < 1'),
    I('ln(1 + x) = x − x^{2}/2 + x^{3}/3 − …',note='−1 < x ≤ 1'),
    I('tan^{−1} x = x − x^{3}/3 + x^{5}/5 − …',note='|x| ≤ 1'),
    I('(1 + x)^{a} = 1 + ax + a(a − 1)x^{2}/2! + …',note='|x| < 1')),
  wide=True))

vector=SH('vector calculus','Vector Calculus','vector calculus',
 S('Operators',
  G('In x, y, z',
    I('∇f = (∂f/∂x, ∂f/∂y, ∂f/∂z)',note='gradient'),
    I('∇ · F = ∂F_{x}/∂x + ∂F_{y}/∂y + ∂F_{z}/∂z',note='divergence'),
    I('∇ × F = (∂F_{z}/∂y − ∂F_{y}/∂z, ∂F_{x}/∂z − ∂F_{z}/∂x, ∂F_{y}/∂x − ∂F_{x}/∂y)',note='curl'),
    I('∇^{2} f = ∂^{2}f/∂x^{2} + ∂^{2}f/∂y^{2} + ∂^{2}f/∂z^{2}',note='Laplacian'),
    I('∇ × (∇f) = 0'),
    I('∇ · (∇ × F) = 0')),
  wide=True),
 S('Theorems',
  G('Integrals',
    I('∫_{C} ∇f · dr = f(b) − f(a)',note='gradient theorem'),
    I('∮_{C} (P dx + Q dy) = ∬_{D} (∂Q/∂x − ∂P/∂y) dA',note='Green'),
    I('∮_{C} F · dr = ∬_{S} (∇ × F) · dS',note='Stokes'),
    I('∯_{S} F · dS = ∭_{V} (∇ · F) dV',note='divergence theorem')),
  wide=True))

continuum=SH('continuum mechanics','Continuum Mechanics','continuum mechanics: stress, beams, torsion, fluids',
 S('Stress and strain',
  G('Simple loading',
    I('σ = F/A',note='normal stress'),
    I('τ = V/A',note='average shear stress'),
    I('ε = ΔL/L',note='normal strain'),
    I('γ = Δx/h',note='shear strain, small angle')),
  G('As tensors',
    I('t_{i} = σ_{ij} n_{j}',note='traction, Cauchy'),
    I('σ_{ij} = σ_{ji}',note='symmetric'),
    I('ε_{ij} = (1/2)(∂u_{i}/∂x_{j} + ∂u_{j}/∂x_{i})',note='small strain'),
    I('p = −σ_{kk}/3',note='mean pressure'),
    I('s_{ij} = σ_{ij} − (σ_{kk}/3) δ_{ij}',note='deviatoric stress'),
    I('ΔV/V = ε_{kk}',note='dilatation')),
  G('Plane stress: rotating by θ',
    I('σ_{x′} = (σ_{x} + σ_{y})/2 + ((σ_{x} − σ_{y})/2) cos(2θ) + τ_{xy} sin(2θ)'),
    I('τ_{x′y′} = −((σ_{x} − σ_{y})/2) sin(2θ) + τ_{xy} cos(2θ)')),
  G('Plane stress: principal values',
    I('σ_{1,2} = (σ_{x} + σ_{y})/2 ± √(((σ_{x} − σ_{y})/2)^{2} + τ_{xy}^{2})',note='principal stresses'),
    I('τ_{max} = √(((σ_{x} − σ_{y})/2)^{2} + τ_{xy}^{2})',note='in plane'),
    I('tan(2θ_{p}) = 2τ_{xy}/(σ_{x} − σ_{y})',note='principal angle')),
  G('Yield',
    I('σ_{v} = √(((σ_{1} − σ_{2})^{2} + (σ_{2} − σ_{3})^{2} + (σ_{3} − σ_{1})^{2})/2)',note='von Mises'),
    I('σ_{v} = √(σ^{2} + 3τ^{2})',note='von Mises, σ and τ alone'),
    I('τ_{max} = (σ_{1} − σ_{3})/2',note='Tresca')),
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
  G('Equilibrium',
    I('∂σ_{ij}/∂x_{j} + ρ b_{i} = ρ a_{i}',note='a = 0 when static')),
  wide=True),
 S('Axial loading and indeterminate problems',
  G('Axial bars',
    I('δ = PL/(AE)',note='elongation'),
    I('k = AE/L',note='stiffness'),
    I('δ_{T} = αΔT L',note='free thermal growth'),
    I('σ = −EαΔT',note='heated, fully restrained')),
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
    I('τ = VQ/(It)',note='Q = first moment above y'),
    I('τ_{max} = 3V/(2A)',note='rectangle'),
    I('τ_{max} = 4V/(3A)',note='solid circle')),
  G('Load, shear, moment, deflection',
    I('dV/dx = −w',note='w acts downward'),
    I('dM/dx = V'),
    I('EI d^{2}v/dx^{2} = M',note='v up'),
    I('EI d^{4}v/dx^{4} = −w'),
    I('P_{cr} = π^{2}EI/(KL)^{2}',note='Euler buckling')),
  G('Sections',
    I('I = bh^{3}/12',note='rectangle'),
    I('I = πd^{4}/64',note='solid circle'),
    I('I = π(d_{o}^{4} − d_{i}^{4})/64',note='hollow circle'),
    I('I = Ī + Ad^{2}',note='parallel axis')),
  G('Largest moment and deflection',
    H('Beam','Moment','Deflection'),
    L('Cantilever, end load P','PL','PL^{3}/(3EI)'),
    L('Cantilever, uniform w','wL^{2}/2','wL^{4}/(8EI)'),
    L('Cantilever, end moment M','M_{0}','M_{0}L^{2}/(2EI)'),
    L('Simply supported, centre load P','PL/4','PL^{3}/(48EI)'),
    L('Simply supported, uniform w','wL^{2}/8','5wL^{4}/(384EI)')),
  G('End slope',
    H('Beam','Slope'),
    L('Cantilever, end load P','PL^{2}/(2EI)'),
    L('Cantilever, uniform w','wL^{3}/(6EI)'),
    L('Simply supported, centre load P','PL^{2}/(16EI)'),
    L('Simply supported, uniform w','wL^{3}/(24EI)')),
  wide=True,leads=[280,120]),
 S('Fluids',
  G('At rest',
    I('dp/dz = −ρg',note='z up'),
    I('p = p_{0} + ρgh',note='h below the surface'),
    I('F_{b} = ρgV',note='buoyancy'),
    I('F = p_{c} A',note='on a flat plate, at its centroid')),
  G('Moving (Bernoulli: steady, incompressible, no friction)',
    I('p + ρv^{2}/2 + ρgz = constant',note='Bernoulli'),
    I('Re = ρvD/μ',note='Reynolds number'),
    I('Q = πR^{4}Δp/(8μL)',note='Poiseuille, laminar pipe'),
    I('h_{f} = f (L/D) v^{2}/(2g)',note='Darcy, head loss'))),
 S('Conservation laws on a control volume',
  G('In general',
    I('dB/dt = d/dt ∫_{CV} ρb dV + ∫_{CS} ρb (v · n) dA',note='Reynolds transport'),
    I('0 = d/dt ∫_{CV} ρ dV + ∫_{CS} ρ (v · n) dA',note='mass'),
    I('Σ F = d/dt ∫_{CV} ρv dV + ∫_{CS} ρv (v · n) dA',note='momentum'),
    I('Σ τ = d/dt ∫_{CV} ρ(r × v) dV + ∫_{CS} ρ(r × v)(v · n) dA',note='angular momentum'),
    I('Q̇ − Ẇ = d/dt ∫_{CV} ρe dV + ∫_{CS} ρ(e + p/ρ)(v · n) dA',note='energy, e = u + v²/2 + gz')),
  G('Steady, one way in and one out',
    I('ṁ = ρ_{1} A_{1} v_{1} = ρ_{2} A_{2} v_{2}',note='mass'),
    I('Σ F = ṁ (v_{2} − v_{1})',note='momentum, as vectors'),
    I('Q̇ − Ẇ = ṁ (h_{2} − h_{1} + (v_{2}^{2} − v_{1}^{2})/2 + g(z_{2} − z_{1}))',note='energy, h enthalpy')),
  G('At a point',
    I('∂ρ/∂t + ∇ · (ρv) = 0',note='continuity'),
    I('ρ Dv/Dt = −∇p + μ∇^{2}v + ρg',note='Navier-Stokes, incompressible')),
  wide=True))

precalc=SH('precalculus','Precalculus','precalculus',
 S('Quadratics',
  G('Solving',
    I('x = (−b ± √(b^{2} − 4ac))/(2a)',note='quadratic formula'),
    I('Δ = b^{2} − 4ac',note='discriminant'),
    I('x = −b/(2a)',note='vertex'),
    I('y = a(x − h)^{2} + k',note='vertex form')),
  G('Lines',
    I('y = m x + b',note='slope-intercept'),
    I('y − y_{1} = m(x − x_{1})',note='point-slope'),
    I('m_{1} m_{2} = −1',note='perpendicular'))),
 S('Conic sections',
  G('Centred at (h, k) or the origin',
    I('(x − h)^{2} + (y − k)^{2} = r^{2}',note='circle'),
    I('x^{2}/a^{2} + y^{2}/b^{2} = 1',note='ellipse'),
    I('x^{2}/a^{2} − y^{2}/b^{2} = 1',note='hyperbola'),
    I('y^{2} = 4 p x',note='parabola')),
  wide=True),
 S('Sequences and logs',
  G('Sequences',
    I('a_{n} = a_{1} + (n − 1) d',note='arithmetic'),
    I('S_{n} = n(a_{1} + a_{n})/2',note='arithmetic sum'),
    I('a_{n} = a_{1} r^{n−1}',note='geometric'),
    I('S_{n} = a_{1}(1 − r^{n})/(1 − r)',note='geometric sum'),
    I('S = a_{1}/(1 − r)',note='infinite, |r| < 1')),
  G('Logs',
    I('log_{b}(x) = ln(x)/ln(b)',note='change of base'),
    I('log_{b}(xy) = log_{b}(x) + log_{b}(y)'),
    I('log_{b}(x^{n}) = n log_{b}(x)'))))

diffeq=SH('differential equations','Differential Equations','differential equations',
 S('First order',
  G('Linear and separable',
    I('dy/dt + p y = q',note='linear'),
    I('μ = e^{∫ p dt}',note='integrating factor'),
    I('y = (1/μ)(∫ μ q dt + C)',note='solution'),
    I('∫ dy/g(y) = ∫ f(t) dt',note='separable'),
    I('y = y_{0} e^{kt}',note='dy/dt = k y')),
  wide=True),
 S('Second order',
  G('Constant coefficients',
    I('a y′′ + b y′ + c y = 0'),
    I('a r^{2} + b r + c = 0',note='characteristic equation')),
  G('Solution by roots',
    H('Roots','Solution'),
    L('Real and distinct','y = c_{1} e^{r_{1}t} + c_{2} e^{r_{2}t}'),
    L('Repeated','y = (c_{1} + c_{2} t) e^{rt}'),
    L('Complex, α ± jβ','y = e^{αt} (c_{1} cos(βt) + c_{2} sin(βt))')),
  G('More',
    I('W = y_{1} y′_{2} − y_{2} y′_{1}',note='Wronskian'),
    I('x′ = A x',note='system'),
    I('x = v e^{λt}',note='A v = λ v')),
  wide=True,leads=[180]),
 S('Partial differential equations',
  G('The classics',
    I('∂u/∂t = α ∂^{2}u/∂x^{2}',note='heat equation'),
    I('∂^{2}u/∂t^{2} = c^{2} ∂^{2}u/∂x^{2}',note='wave equation'),
    I('∂^{2}u/∂x^{2} + ∂^{2}u/∂y^{2} = 0',note='Laplace equation')),
  wide=True))

numerical=SH('numerical methods','Numerical Methods','numerical methods',
 S('Roots',
  G('Iterations',
    I('x_{n+1} = x_{n} − f(x_{n})/f′(x_{n})',note='Newton'),
    I('x_{n+1} = x_{n} − f(x_{n})(x_{n} − x_{n−1})/(f(x_{n}) − f(x_{n−1}))',note='secant'),
    I('|e_{n}| ≤ (b − a)/2^{n}',note='bisection')),
  wide=True),
 S('Derivatives and integrals',
  G('Finite differences',
    I('f′(x) ≈ (f(x + h) − f(x))/h',note='forward'),
    I('f′(x) ≈ (f(x + h) − f(x − h))/(2h)',note='central'),
    I('f′′(x) ≈ (f(x + h) − 2f(x) + f(x − h))/h^{2}',note='second')),
  G('Integrals',
    I('∫ f dx ≈ (h/2)(f_{0} + 2f_{1} + 2f_{2} + … + f_{n})',note='trapezoid rule'),
    I('∫ f dx ≈ (h/3)(f_{0} + 4f_{1} + 2f_{2} + 4f_{3} + … + f_{n})',note='Simpson, n even')),
  wide=True),
 S('ODE solvers',
  G('Stepping',
    I('y_{n+1} = y_{n} + h f(t_{n}, y_{n})',note='Euler'),
    I('k_{1} = h f(t_{n}, y_{n})',note='RK4'),
    I('k_{2} = h f(t_{n} + h/2, y_{n} + k_{1}/2)',note='RK4'),
    I('k_{3} = h f(t_{n} + h/2, y_{n} + k_{2}/2)',note='RK4'),
    I('k_{4} = h f(t_{n} + h, y_{n} + k_{3})',note='RK4'),
    I('y_{n+1} = y_{n} + (k_{1} + 2k_{2} + 2k_{3} + k_{4})/6',note='RK4')),
  wide=True),
 S('Linear systems and error',
  G('Matrices',
    I('x = (A^{T} A)^{−1} A^{T} b',note='least squares'),
    I('κ(A) = ‖A‖ ‖A^{−1}‖',note='condition number')),
  G('Error',
    I('|x − x_{a}|/|x|',note='relative error'),
    I('|x − x_{a}|',note='absolute error'))))

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
class Pic:
    "a picture in points, y up; its box fits what's drawn, whatever the origin"
    SCALE=1.25
    def __init__(s,*_,**__): s.ops=[]; s.box=[]
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
    def arrow(s,a,b,w=0.7,head=3.2,both=False):
        head/=s.SCALE
        dx,dy=b[0]-a[0],b[1]-a[1]; L=math.hypot(dx,dy); ux,uy=dx/L,dy/L
        def tip(p,ux,uy):
            s.path([p,(p[0]-ux*head-uy*head*.42,p[1]-uy*head+ux*head*.42),(p[0]-ux*head+uy*head*.42,p[1]-uy*head-ux*head*.42)],closed=True,fill='black',stroke=False)
        a2=(a[0]+ux*head*.8,a[1]+uy*head*.8) if both else a
        s.line(a2,(b[0]-ux*head*.8,b[1]-uy*head*.8),w)
        tip(b,ux,uy)
        if both: tip(a,-ux,-uy)
    def label(s,p,m,size=7):
        # roughly the label's own size, so the box holds it
        wide=len(re.sub(r'hat\(|\)|_|"|arrow\(','',m))*size*0.3+size*0.3
        s.grow(p,wide,size*0.55)
        s.ops.append(lambda T,P=p: 'place(center + horizon, dx: %s, dy: %s, text(size: %gpt, $%s$))'%(*T(P,True),size,m))
    def dim(s,a,b,m,off=(0,0)):
        "a measure: arrows both ways, its label beside the middle"
        s.arrow(a,b,w=0.45,head=2.4,both=True)
        s.label(((a[0]+b[0])/2+off[0],(a[1]+b[1])/2+off[1]),m,6.5)
    def dot(s,p,r=1.4):
        s.grow(p,r,r)
        s.ops.append(lambda T,P=p: 'place(center + horizon, dx: %s, dy: %s, circle(radius: %gpt, fill: black, stroke: none))'%(*T(P,True),r))
    def cm(s,p): s.dot(p,1.8)
    def axes(s,p,L=13,z=True,which='xyz'):
        if 'x' in which: s.arrow(p,(p[0]+L,p[1])); s.label((p[0]+L+5,p[1]+0.5),'hat(b)_x',6.5)
        if 'y' in which: s.arrow(p,(p[0],p[1]+L)); s.label((p[0]+0.5,p[1]+L+5),'hat(b)_y',6.5)
        if z and 'z' in which:
            e=proj(0,0,L*2.1); s.arrow(p,(p[0]+e[0],p[1]+e[1])); s.label((p[0]+e[0]-5,p[1]+e[1]-3),'hat(b)_z',6.5)
    def typst(s):
        pad=2; x0=min(x for x,_ in s.box)-pad; x1=max(x for x,_ in s.box)+pad
        y0=min(y for _,y in s.box)-pad; y1=max(y for _,y in s.box)+pad
        w,h=x1-x0,y1-y0
        def T(p,pair=False):
            x,y=s.at(p); x,y=x-x0,y1-y
            # a pair: from the box's centre, for what's placed by its own centre
            return ('%.1fpt'%(x-w/2),'%.1fpt'%(y-h/2)) if pair else '(%.1fpt, %.1fpt)'%(x,y)
        return '#box(width: %.1fpt, height: %.1fpt, baseline: %.1fpt, {\n%s\n})'%(w,h,h/2-3.3,'\n'.join(op(T) for op in s.ops))

def pic_rod():
    p=Pic(120,44,oy=26); p.path([(-48,-1.4),(48,-1.4),(48,1.4),(-48,1.4)],closed=True,fill=GREY,w=0.6)
    p.axes((0,0),z=True); p.cm((0,0)); p.dim((-48,-11),(48,-11),'L',(0,-5)); return p
def pic_arc(fill=False,sector=False):
    r=40; th=50; p=Pic(120,78,oy=66); O=(0,-44)
    a0,a1=90-th,90+th; arc=ell(O[0],O[1],r,r,a0,a1)
    if sector: p.path([O]+arc,closed=True,fill=GREY,w=0.8)
    else:
        p.path(arc,w=1.6)
        for e in (arc[0],arc[-1]): p.line(O,e,w=0.4,dash='dashed')
    yc=(2*r*math.sin(math.radians(th))/(3*math.radians(th))) if sector else r*math.sin(math.radians(th))/math.radians(th)
    p.line(O,(0,O[1]+r),w=0.4,dash='dotted')
    c=(0,O[1]+yc); p.axes(c,L=11); p.cm(c)
    p.path(ell(O[0],O[1],10,10,90,a1),w=0.4); p.label((O[0]-7,O[1]+15),'θ',6.5)
    p.path(ell(O[0],O[1],10,10,a0,90),w=0.4); p.label((O[0]+7,O[1]+15),'θ',6.5)
    e=arc[0]; p.label(((O[0]+e[0])/2+5,(O[1]+e[1])/2-4),'r',6.5)
    return p
def pic_semi(fill=False):
    r=36; p=Pic(110,58,oy=48); O=(0,-36)
    arc=ell(O[0],O[1],r,r,0,180)
    if fill: p.path(arc,closed=True,fill=GREY,w=0.8)
    else: p.path(arc,w=1.6); p.line((-r,O[1]),(r,O[1]),w=0.4,dash='dashed')
    c=(0,O[1]+(4*r/(3*math.pi) if fill else 2*r/math.pi)); p.axes(c,L=11); p.cm(c)
    p.line(O,(-r*0.6,O[1]+r*0.8),w=0.4); p.label((-r*0.3-5,O[1]+r*0.4-2),'r',6.5)
    return p
def pic_circle(fill=False,a=30,b=30):
    p=Pic(2*a+40,2*b+16); pts=ell(0,0,a,b)
    if fill: p.path(pts,closed=True,fill=GREY,w=0.8)
    else: p.path(pts,closed=True,w=1.6)
    p.axes((0,0),L=12); p.cm((0,0))
    if a==b: p.line((0,0),(a*0.7,b*0.7),w=0.4); p.label((a*0.45+4,b*0.45-3),'r',6.5)
    else:
        p.line((0,0),(-a,0),w=0.4,dash='dotted'); p.label((-a/2,3.5),'a',6.5)
        p.line((0,0),(0,-b),w=0.4,dash='dotted'); p.label((3.5,-b/2),'b',6.5)
    return p
def pic_plate(a=56,b=36):
    p=Pic(a+44,b+22,oy=b/2+12)
    p.path([(-a/2,-b/2),(a/2,-b/2),(a/2,b/2),(-a/2,b/2)],closed=True,fill=GREY,w=0.8)
    p.axes((0,0),L=12); p.cm((0,0)); p.label((0,-b/2-5),'a',6.5); p.label((a/2+5,-b/4),'b',6.5); return p
def cyl_outline(p,r,h,inner=None,fill=GREY,shell=False):
    f=proj(0,0,h/2); bk=proj(0,0,-h/2)
    # the side: the two tangent lines, perpendicular to the axis's slant
    d=(f[0]-bk[0],f[1]-bk[1]); L=math.hypot(*d); n=(-d[1]/L*r,d[0]/L*r)
    ang=math.degrees(math.atan2(n[1],n[0]))
    side=ell(bk[0],bk[1],r,r,ang,ang+180,24)+ell(f[0],f[1],r,r,ang+180,ang,24)
    p.path(side,closed=True,fill=GREY if shell or inner else 'rgb(128, 128, 128, 40)',w=0.8)
    if shell: p.path(ell(bk[0],bk[1],r,r,ang+180,ang+360,24),w=0.4,dash='dashed')
    front=ell(f[0],f[1],r,r)
    if inner: p.path(front,closed=True,fill=GREY,w=0.8,holes=[ell(f[0],f[1],inner,inner)])
    elif shell: p.path(front,closed=True,w=0.8)
    else: p.path(front,closed=True,fill=GREY,w=0.8)
    return f
def pic_cyl(kind='solid'):
    r=24; h=64; p=Pic(122,82,ox=66,oy=40)
    f=cyl_outline(p,r,h,inner=14 if kind=='thick' else None,shell=kind=='thin')
    p.axes((0,0),L=12,z=False); e=proj(0,0,h/2+22); p.arrow((0,0),e); p.label((e[0]-5,e[1]-3),'hat(b)_z',6.5)
    p.cm((0,0))
    if kind=='thick':
        p.line(f,(f[0]+14*0.7,f[1]+14*0.7),w=0.4); p.label((f[0]+4,f[1]+11),'r_i',6)
        p.line(f,(f[0]+r*0.97,f[1]-r*0.26),w=0.4); p.label((f[0]+r+4,f[1]-r*0.3),'r_o',6)
    else: p.line(f,(f[0]+r*0.97,f[1]-r*0.26),w=0.4); p.label((f[0]+r*0.5,f[1]-r*0.13-4),'r',6.5)
    t=proj(r*0.0,-r,0); a=(f[0]+t[0],f[1]+t[1]); b=(a[0]-proj(0,0,h)[0],a[1]-proj(0,0,h)[1])
    p.label(((a[0]+b[0])/2+4,(a[1]+b[1])/2-6),'h',6.5)
    return p
def pic_cone(thin=False):
    r=30; h=60; p=Pic(110,84,oy=50); base=-h/4 if not thin else -h/3; ry=8
    apex=(0,base+h)
    if thin: p.path(ell(0,base,r,ry,0,180),w=0.5,dash='dashed'); p.path(ell(0,base,r,ry,180,360)+[(r,base),apex,(-r,base)],w=0.8)
    else: p.path([(-r,base),apex,(r,base)]+ell(0,base,r,ry,360,180),closed=True,fill=GREY,w=0.8); p.path(ell(0,base,r,ry,0,180),w=0.4,dash='dashed')
    p.axes((0,0),L=12); p.cm((0,0)); p.line((0,base),(r,base),w=0.4); p.label((r*0.55,base-ry-4),'r',6.5)
    p.dim((-r-7,base),(-r-7,base+h),'h',(-4.5,0))
    return p
def pic_sphere(fill=True,a=30,b=30,c=10):
    p=Pic(2*a+38,2*b+14);
    p.path(ell(0,0,a,b),closed=True,fill=GREY if fill else None,w=0.8 if fill else 1.4)
    p.path(ell(0,0,a,c,180,360),w=0.5); p.path(ell(0,0,a,c,0,180),w=0.4,dash='dashed')
    p.axes((0,0),L=12); p.cm((0,0))
    if a==b: p.line((0,0),(-a*0.68,b*0.73),w=0.4); p.label((-a*0.34-4,b*0.37+3),'r',6.5)
    return p
def pic_ellipsoid(fill=True):
    a,b,c=34,24,9; p=Pic(2*a+40,2*b+22,oy=b+10)
    p.path(ell(0,0,a,b),closed=True,fill=GREY if fill else None,w=0.8 if fill else 1.4)
    p.path(ell(0,0,a,c,180,360),w=0.5); p.path(ell(0,0,a,c,0,180),w=0.4,dash='dashed')
    p.axes((0,0),L=11); p.cm((0,0))
    p.dim((0,-b-5),(a,-b-5),'a',(0,-4)); p.dim((a+6,0),(a+6,b),'b',(4,0)); p.label((a-6,-c+1),'c',6)
    return p
def pic_hemi():
    r=34; p=Pic(100,64,oy=46); B=(0,-36)
    p.path(ell(B[0],B[1],r,r,0,180)+ell(B[0],B[1],r,9,180,360),closed=True,fill=GREY,w=0.8)
    p.path(ell(B[0],B[1],r,9,0,180),w=0.4,dash='dashed')
    c=(0,B[1]+3*r/8); p.axes(c,L=11); p.cm(c); p.dot(B); p.label((B[0]+8,B[1]-4),'B_o',6)
    p.line(B,(r*0.6,B[1]+r*0.8),w=0.4); p.label((r*0.42+4,B[1]+r*0.55),'r',6.5)
    return p
def pic_parab():
    r=26; a=64; p=Pic(112,70,ox=26);
    prof=[(a*(1-(t/r)**2),t) for t in [r*i/24 for i in range(-24,25)]]
    p.path(prof,closed=True,fill=GREY,w=0.8)
    p.path(ell(0,0,7,r,90,270),w=0.8)
    c=(a/3,0); p.axes(c,L=12); p.cm(c); p.dot((0,0)); p.label((-14,0),'B_o',6)
    p.label((-12,r*0.6),'r',6.5); p.dim((0,-r-5),(a,-r-5),'a',(0,-4)); return p
def pic_box(particles=False):
    a,b,c=60,30,40; p=Pic(116,82,ox=58,oy=40)
    def P(x,y,z): return proj(x,y,z)
    xs,ys,zs=(-a/2,a/2),(-b/2,b/2),(-c/2,c/2)
    V={(i,j,k):P(xs[i],ys[j],zs[k]) for i in (0,1) for j in (0,1) for k in (0,1)}
    if not particles:
        for face in [[(0,0,1),(1,0,1),(1,1,1),(0,1,1)],[(1,0,1),(1,0,0),(1,1,0),(1,1,1)],[(0,1,1),(1,1,1),(1,1,0),(0,1,0)]]:
            p.path([V[q] for q in face],closed=True,fill=GREY,w=0.8)
    else:
        edges=[(u,v) for u in V for v in V if u<v and sum(x!=y for x,y in zip(u,v))==1]
        for u,v in edges: p.line(V[u],V[v],w=0.5)
        for q in V.values(): p.dot(q,2.2)
    p.axes((0,0),L=12); p.cm((0,0))
    lab=('2a','2b','2c') if particles else ('a','b','c')
    p.label(((V[0,0,1][0]+V[1,0,1][0])/2,V[0,0,1][1]-5),lab[0],6.5)
    p.label((V[1,0,1][0]+5,(V[1,0,1][1]+V[1,1,1][1])/2),lab[1],6.5)
    m=((V[1,0,1][0]+V[1,0,0][0])/2,(V[1,0,1][1]+V[1,0,0][1])/2); p.label((m[0]+5,m[1]-3),lab[2],6.5)
    return p
def pic_triangle():
    p=Pic(110,70,ox=12,oy=56); O=(0,0); P_=(56,-48); Q=(88,8)
    p.path([O,P_,Q],closed=True,fill=GREY,w=0.5)
    p.arrow(O,P_,w=0.9); p.arrow(O,Q,w=0.9); p.dot(O)
    c=((P_[0]+Q[0])/3,(P_[1]+Q[1])/3); p.cm(c)
    p.label((-5,3),'O',6.5); p.label((P_[0]+5,P_[1]-2),'P',6.5); p.label((Q[0]+4,Q[1]+4),'Q',6.5)
    p.label((P_[0]/2-6,P_[1]/2-4),'arrow(p)',7); p.label((Q[0]/2,Q[1]/2+7),'arrow(q)',7); return p
def pic_tetra():
    p=Pic(110,82,ox=12,oy=58); O=(0,0); P_=(52,-50); Q=(88,0); R=(60,20)
    p.path([O,P_,Q],closed=True,fill=GREY,w=0.4); p.path([P_,Q,R],closed=True,fill=GREY,w=0.4)
    p.line(O,R,w=0.4);
    for e,n,off in [(P_,'p',(-7,-4)),(Q,'q',(2,-6)),(R,'r',(-6,5))]:
        p.arrow(O,e,w=0.9); p.label((e[0]/2+off[0],e[1]/2+off[1]),'arrow(%s)'%n,7)
    p.dot(O); p.label((-5,3),'O',6.5); p.label((P_[0]+5,P_[1]-2),'P',6.5); p.label((Q[0]+5,Q[1]),'Q',6.5); p.label((R[0]+2,R[1]+5),'R',6.5)
    return p

def Pc(pic,show,note=None):
    "a body's picture, then its volume (or the first of its formulas): the volume is what a click copies"
    d=I(show,note=note); d['pic']=pic.typst(); return d
def U(name,value):
    "a constant: its name in words, its value as a value with its unit"
    d=L(name,value); d['unit']=True; return d
def W(text):
    "a line of words: drawn in the app's font, copied as it is"
    return {'show':text,'copy':text,'words':True}
def body_group(title,pic,first,*rows,note=None):
    return G(title,Pc(pic,first,note),*rows)

# Mitiguy's notation: ^{N}v⃗^{Q} is Q's velocity in N, I⃡ an inertia dyadic, B∕O (a division slash) "B about O"
dynamics=SH('dynamics','Statics and Dynamics','statics and dynamics',
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
  G('Angular velocity',
    I('^{A}dv⃗/dt = ^{B}dv⃗/dt + ^{A}ω⃗^{B} × v⃗',note='golden rule'),
    I('^{A}ω⃗^{B} = ±θ̇ λ̂',note='λ̂ fixed in both A and B'),
    I('^{A}ω⃗^{D} = ^{A}ω⃗^{B} + ^{B}ω⃗^{C} + ^{C}ω⃗^{D}',note='addition theorem'),
    I('^{A}ω⃗^{B} = −^{B}ω⃗^{A}')),
  G('Angular acceleration',
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
    I('I_{xy} = I_{xz} = I_{yz} = 0',note='about the mass centre, on the axes drawn'),
    I('I = Ī + m d^{2}',note='parallel axis')),
  G('About the centre, unless it says',
    H('Body','I'),
    L('Solid sphere','(2/5) m r^{2}'),
    L('Hollow sphere','(2/3) m r^{2}'),
    L('Solid cylinder','(1/2) m r^{2}'),
    L('Hoop','m r^{2}'),
    L('Rod','m L^{2}/12'),
    L('Rod, about one end','m L^{2}/3')),
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
    I('^{N}K^{B} = (1/2) ^{N}v⃗^{B_{p}} · ^{N}L⃗^{B} + (1/2) ^{N}ω⃗^{B} · ^{N}H⃗^{B∕B_{p}}',note='Bₚ the mass centre, or fixed in B and N'),
    I('^{N}K^{B} = (1/2) m^{B} ^{N}v⃗^{B_{p}} · ^{N}v⃗^{B_{p}} + (1/2) I_{kk}^{B∕B_{p}} (^{N}ω^{B})^{2}',note='simple 2D, Bₚ as above')),
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
    I('^{N}P^{S} = Σ_{i=1}^{ν} F⃗^{Q_{i}} · ^{N}v⃗^{Q_{i}}',note='a set of forces'),
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
    I('Σ F = 0'),
    I('Σ M = 0',note='about any point'),
    I('M = F d',note='moment'),
    I('f ≤ μ N',note='friction')),
  G('Centroids',
    I('x̄ = (Σ A_{i} x_{i})/(Σ A_{i})',note='composite area'),
    I('I = Ī + A d^{2}',note='parallel axis'))),
 S('Motion',
  G('Particles',
    I('a = v dv/ds'),
    I('a_{n} = v^{2}/ρ',note='normal, radius ρ'),
    I('W = Δ KE',note='work-energy'),
    I('∫ F dt = Δp',note='impulse-momentum'),
    I('e = (v′_{2} − v′_{1})/(v_{1} − v_{2})',note='restitution')),
  G('Rotation and gears',
    I('v = ω r',note='rolling'),
    I('Σ M = I α'),
    I('ω_{1}/ω_{2} = N_{2}/N_{1}',note='gear ratio'),
    I('τ_{2}/τ_{1} = N_{2}/N_{1}',note='gear ratio'))),
 S('Vibrations',
  G('Mass, spring and damper',
    I('m x′′ + c x′ + k x = 0'),
    I('ω_{n} = √(k/m)',note='natural frequency'),
    I('f_{n} = ω_{n}/(2π)'),
    I('c_{c} = 2√(k m)',note='critical damping'),
    I('ζ = c/c_{c}',note='damping ratio'),
    I('ω_{d} = ω_{n} √(1 − ζ^{2})',note='damped frequency'),
    I('δ = ln(x_{1}/x_{2})',note='logarithmic decrement'),
    I('ω_{n} = √(g/L)',note='simple pendulum'),
    I('ω_{n} = √(m g d/I_{O})',note='physical pendulum, pivot O')),
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

heat=SH('heat transfer','Heat Transfer','heat transfer',
 S('Modes',
  G('Rates',
    I('Q̇ = −k A dT/dx',note='conduction, Fourier'),
    I('Q̇ = h A (T − T_{∞})',note='convection, Newton'),
    I('Q̇ = ε σ A (T_{1}^{4} − T_{2}^{4})',note='radiation')),
  G('Thermal resistance',
    I('R = L/(k A)',note='conduction'),
    I('R = 1/(h A)',note='convection'),
    I('1/(U A) = Σ R',note='in series'),
    I('Q̇ = ΔT/R'))),
 S('Unsteady and exchangers',
  G('Lumped capacity',
    I('T − T_{∞} = (T_{0} − T_{∞}) e^{−t/τ}'),
    I('τ = ρ V c/(h A)'),
    I('Bi = h L/k',note='lumped if under 0.1')),
  G('Heat exchangers',
    I('Q̇ = U A ΔT_{m}'),
    I('ΔT_{m} = (ΔT_{1} − ΔT_{2})/ln(ΔT_{1}/ΔT_{2})',note='log mean'))),
 S('Convection numbers',
  G('Dimensionless',
    I('Nu = h L/k',note='Nusselt'),
    I('Re = ρ v L/μ',note='Reynolds'),
    I('Pr = μ c_{p}/k',note='Prandtl'),
    I('Nu = 0.023 Re^{0.8} Pr^{n}',note='Dittus-Boelter, turbulent pipe')),
  wide=True))

compsci=SH('computer science','Computer Science','computer science',
 S('Algorithms',
  G('Sums and recurrences',
    I('Σ_{i=1}^{n} i = n(n + 1)/2'),
    I('Σ_{i=0}^{n} 2^{i} = 2^{n+1} − 1'),
    I('T(n) = a T(n/b) + f(n)',note='master theorem'),
    I('T(n) = Θ(n^{log_{b} a})',note='when the leaves dominate'),
    I('T(n) = Θ(n^{log_{b} a} log n)',note='when levels are equal'),
    I('T(n) = Θ(f(n))',note='when f dominates, if regular')),
  G('Hashing',
    I('α = n/m',note='load factor'),
    I('P ≈ 1 − e^{−n^{2}/(2m)}',note='birthday collision')),
  wide=True),
 S('Information',
  G('Bits',
    I('N = 2^{n}',note='values in n bits'),
    I('n = log_{2}(N)',note='bits needed'),
    I('H = −Σ p_{i} log_{2}(p_{i})',note='entropy')),
  wide=True),
 S('Systems',
  G('Performance',
    I('S = 1/((1 − p) + p/s)',note='Amdahl'),
    I('t = N CPI/f',note='CPU time'),
    I('AMAT = t_{hit} + m t_{miss}',note='average memory access')),
  G('Networks',
    I('t = L/R',note='transmission delay'),
    I('t = d/s',note='propagation delay'),
    I('B = R RTT',note='bandwidth-delay product')),
  wide=True))

mlsheet=SH('machine learning','Machine Learning','machine learning',
 S('Models and losses',
  G('Models',
    I('ŷ = w · x + b',note='linear'),
    I('w = (X^{T} X)^{−1} X^{T} y',note='normal equation'),
    I('σ(z) = 1/(1 + e^{−z})',note='sigmoid'),
    I('p_{i} = e^{z_{i}}/(Σ_{j} e^{z_{j}})',note='softmax'),
    I('f(x) = max(0, x)',note='ReLU')),
  G('Losses',
    I('MSE = (1/n) Σ (y_{i} − ŷ_{i})^{2}'),
    I('L = −Σ y_{i} ln(ŷ_{i})',note='cross-entropy'),
    I('D = Σ p_{i} ln(p_{i}/q_{i})',note='KL divergence'),
    I('L + λ ‖w‖^{2}',note='L2 regularization'))),
 S('Training and scoring',
  G('Training',
    I('w ← w − η ∇L',note='gradient descent')),
  G('Scoring',
    I('P = TP/(TP + FP)',note='precision'),
    I('R = TP/(TP + FN)',note='recall'),
    I('F_{1} = 2PR/(P + R)',note='F1 score'),
    I('A = (TP + TN)/(TP + TN + FP + FN)',note='accuracy'),
    I('cos(θ) = (a · b)/(|a| |b|)',note='cosine similarity'))))

astronomy=SH('astronomy','Astronomy','astronomy',
 S('Orbits and gravity',
  G('Orbits',
    I('T^{2} = 4π^{2} a^{3}/(G M)',note='Kepler'),
    I('v = √(G M/r)',note='circular orbit'),
    I('v_{e} = √(2 G M/r)',note='escape velocity'),
    I('v^{2} = G M (2/r − 1/a)',note='vis-viva')),
  G('Gravity',
    I('g = G M/r^{2}',note='surface gravity'),
    I('r_{s} = 2 G M/c^{2}',note='Schwarzschild radius'))),
 S('Stars and distance',
  G('Light',
    I('L = 4π R^{2} σ T^{4}',note='Stefan-Boltzmann'),
    I('λ_{max} = b/T',note='Wien'),
    I('F = L/(4π d^{2})',note='flux'),
    I('m − M = 5 log_{10}(d) − 5',note='distance modulus, d in parsecs')),
  G('Distance',
    I('d = 1/p',note='parallax, arcsec to parsecs'),
    I('v = H_{0} d',note='Hubble'),
    I('z = Δλ/λ',note='redshift'))))

finance=SH('finance','Finance Formulas','finance formulas',
 S('Interest and time value',
  G('Growth',
    I('A = P(1 + r t)',note='simple interest'),
    I('A = P(1 + r/n)^{n t}',note='compound interest'),
    I('A = P e^{r t}',note='continuous'),
    I('t ≈ 72/r',note='rule of 72, r in percent')),
  G('Present value',
    I('PV = FV/(1 + r)^{n}'),
    I('NPV = Σ C_{t}/(1 + r)^{t}',note='net present value'),
    I('CAGR = (V_{f}/V_{i})^{1/n} − 1',note='growth rate'))),
 S('Payments and returns',
  G('Payments',
    I('FV = PMT ((1 + r)^{n} − 1)/r',note='annuity'),
    I('PV = PMT (1 − (1 + r)^{−n})/r',note='annuity'),
    I('M = P r/(1 − (1 + r)^{−n})',note='loan payment')),
  G('Business',
    I('ROI = gain/cost',note='return on investment'),
    I('Q = F/(p − v)',note='break-even units'),
    I('r_{real} = (1 + r)/(1 + i) − 1',note='after inflation'))))

native=[precalc,diffeq,numerical,dynamics,heat,compsci,mlsheet,astronomy,finance,chemistry,biology,continuum,fourier,euler,complexn,ct,dt,laplace,ztr,circle,geom,stats,linalg,physics,circuits,taylor,vector]
def row(cmd,what): return {'lead':[cmd],'show':what,'copy':cmd,'open':cmd}
index=SH('formulas','Formulas','all the formula sheets',
 S('Click one to open it, or type its name',
  G('Signals and transforms',row('fourier','Fourier series'),row('euler',"Euler's formulas"),row('complex numbers','special values, polar form'),
    row('ct transform table','CT Fourier transforms'),row('dt transform table','DT Fourier transforms'),row('laplace','Laplace transforms'),row('z transform','Z transforms')),
  G('Math',row('identities','trig, logs, algebra, sums'),row('calculus','derivatives and integrals'),row('unit circle','exact trig values'),row('taylor series','Maclaurin series'),row('precalculus','quadratics, conics, sequences'),row('differential equations','ODEs and PDEs'),row('numerical methods','roots, integrals, RK4'),
    row('linear algebra','matrices, vectors, eigenvalues'),row('vector calculus','operators and theorems'),row('geometry','area, volume, triangles')),
  G('Science and engineering',row('physics','mechanics, E&M, quantum'),row('chemistry','gases, equilibrium, kinetics'),row('biology','genetics, enzymes, populations'),row('continuum mechanics','stress, beams, torsion, fluids'),row('electrical engineering','AC, transistors, logic'),row('dynamics','vectors, inertia, Euler, energy'),row('heat transfer','conduction, convection'),row('astronomy','orbits, stars, distance'),row('statistics','probability, distributions'),row('computer science','algorithms, bits, systems'),row('machine learning','models, losses, scoring'),row('finance','interest, loans, returns'),
    row('constants','physical constants'),row('conversions','units and metric prefixes'),row('periodic','periodic table')),
  leads=[140]))
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
UPRIGHT={'Cs','Si','Ry','PV','FV','NPV','PMT','ROI','MSE','TP','FP','FN','TN','Nu','Bi','Pr','CPI','RTT','hit','real','miss','and','mW','BW','SR','GBW','SNR','dBm','LSB','PF','OS','to','GS','DS','BE','ox','ov','ph','FS','ref','clk','cq','su','dc','pk','cm','dB','pH','pOH','pK','HA','OH','vap','tot','sp','eV','mV','kJ','mol','RF','CO','HR','SV','MAP','TPR','BMI','app','cat','out','max','cr','lat','CV','CS','along','for','Re','Im','Var','KE','PE','CT','DT','out','in','th','oc','sc','rms','dB','Ptr','FS'}
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
    s=s.replace('x̄','⟪xbar⟫').replace('f′','f\'').replace('0⁺','0^{+}').replace('<N>','⟪angle⟫').replace('…','⟪dots⟫')
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
            out.append({'xbar':'overline(x)','angle':'angle.l N angle.r','dots':'dots','FS':'"F.S."','mdot':'dot(m)','Qdot':'dot(Q)','Wdot':'dot(W)','Ibar':'overline(I)','yhat':'hat(y)'}[w]); i=j+1; continue
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
 (["standard deviation"],['s = √(s^{2})','s^{2} = Σ (x_{i} − x̄)^{2} / (n − 1)']),
 (["Euler buckling","buckling"],['P_{cr} = π^{2}EI/(KL)^{2}']),
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
 (["compound interest"],['A = P(1 + r/n)^{n t}']),
 (["simple interest"],['A = P(1 + r t)']),
 (["loan payment","mortgage payment"],['M = P r/(1 − (1 + r)^{−n})']),
 (["net present value","npv"],['NPV = Σ C_{t}/(1 + r)^{t}']),
 (["rule of 72"],['t ≈ 72/r']),
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
        out.append({'id':'formula: '+names[0],'title':title,'hint':title,
                    'names':names,
                    'sections':[{'title':title,'wide':True,'groups':[{'title':rows[shows[0]][0][0]['title'],'items':items}]}]})
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
for it in ct['sections'][1]['groups'][0]['items'][1:4]: print(it['copy'])
for it in fourier['sections'][0]['groups'][0]['items']: print(it['copy'])
