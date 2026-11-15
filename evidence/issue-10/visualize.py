#!/usr/bin/env python3
"""Throwaway view of retained raw arcs. Requires Pillow; no engine changes."""
import json
import math
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

D = Path(__file__).resolve().parent
traces = {name: json.loads((D / f'{name}.json').read_text())
          for name in ('baseline', 'intervention')}
comparison = json.loads((D / 'comparison.json').read_text())
indices = [594, 595, 596, 597, 598, 599, 600, 601, 602, 603, 1143, 3151]
fontdir = Path('/usr/share/fonts/liberation')

def font(size, bold=False):
    return ImageFont.truetype(str(fontdir / ('LiberationSans-Bold.ttf' if bold else 'LiberationSans-Regular.ttf')), size)

im = Image.new('RGB', (1400, 1190), '#f5f7fa')
draw = ImageDraw.Draw(im)
ink, muted = '#182633', '#536474'
blue, green, orange = '#1769aa', '#16826c', '#c8640c'

def text(x, y, value, size=20, color=ink, bold=False):
    draw.text((x,y),value,font=font(size,bold),fill=color)

text(42,28,'Basin-2 witness: what changed, and what did not',32,bold=True)
text(42,74,'Recorded raw arcs, cropped to each junction. Left and right use identical scales.',21)
text(42,106,'Only the 596/601 slow-vertex motion choice changed. Neither result is a valid skeleton.',21)
text(44,155,'BASELINE',24,bold=True)
text(724,155,'ONE-MOTION EXPERIMENT',24,bold=True)
text(44,195,'Final junction: 595 / 596 / 601',24,bold=True)
text(724,195,'Final junction: 595 / 596 / 601',24,bold=True)


def event(trace, supports):
    return next(e for e in trace['events'] if e and e['kind']=='TriangleCollapse'
                and sorted(c[1] for c in e['constraints']) == sorted(supports))


def clip(p,q,xmin,xmax,ymin,ymax):
    dx,dy=q[0]-p[0],q[1]-p[1]
    lo,hi=0.0,1.0
    for a,b in [(-dx,p[0]-xmin),(dx,xmax-p[0]),(-dy,p[1]-ymin),(dy,ymax-p[1])]:
        if a==0:
            if b<0:return None
        elif a<0:lo=max(lo,b/a)
        else:hi=min(hi,b/a)
        if lo>hi:return None
    return [(p[0]+t*dx,p[1]+t*dy) for t in [lo,hi]]


def panel(name, xy, triple, origin, width_m, center_x, colors, labels, scale_bar, early=False):
    x,y=xy; w,h=632,245
    trace=traces[name]; e=event(trace,triple)
    vertices={v['id']:v for v in trace['vertices']}
    arcs=[a for a in trace['arcs'] if a['traced_vertex'] in e['vertices']]
    assert len(arcs)==3
    scale=w/width_m
    height_m=h/scale
    bounds=(center_x-width_m/2,center_x+width_m/2,-height_m/2,height_m/2)
    def local(p):return (p[0]-origin[0],p[1]-origin[1])
    def pixel(p):return (round((p[0]-bounds[0])*scale),round((bounds[3]-p[1])*scale))
    tile=Image.new('RGB',(w,h),'white'); td=ImageDraw.Draw(tile)
    # Equal x/y scale. Only translation and clipping are applied to coordinates.
    for a in arcs:
        v=vertices[a['traced_vertex']]
        pair=tuple(sorted(indices[s['input_edge']] for s in v['supports']))
        c=colors[pair]
        segment=clip(local(a['start']),local(a['end']),*bounds)
        if segment:td.line([pixel(p) for p in segment],fill=c,width=4)
    for a in arcs:
        v=vertices[a['traced_vertex']]
        pair=tuple(sorted(indices[s['input_edge']] for s in v['supports']))
        u,vv=pixel(local(a['end']))
        td.ellipse((u-6,vv-6,u+6,vv+6),fill='white',outline=colors[pair],width=3)
    ends=[local(a['end']) for a in arcs]
    p,q=max(((p,q) for p in ends for q in ends),key=lambda pq:math.dist(*pq))
    if math.dist(p,q)*scale>15:
        p,q=pixel(p),pixel(q)
        # Measurement guide is offset below the endpoints and is not an arc.
        by=max(p[1],q[1])+34
        td.line((p[0],by,q[0],by),fill='#ae3141',width=2)
        for xx in [p[0],q[0]]:td.line((xx,by-6,xx,by+6),fill='#ae3141',width=2)
    bar=round(scale_bar*scale)
    td.line((24,h-28,24+bar,h-28),fill=ink,width=3)
    for xx in [24,24+bar]:td.line((xx,h-33,xx,h-23),fill=ink,width=2)
    td.text((24,h-59),('1 mm' if early else '0.1 m'),font=font(18),fill=ink)
    td.rectangle((0,0,w-1,h-1),outline='#d5dde5',width=1)
    im.paste(tile,(x,y))
    draw.rectangle((x+8,y+7,x+w-8,y+62),fill='white')
    text(x+15,y+10,labels[0],19,bold=True)
    text(x+15,y+37,labels[1],18,color=muted)
    return e

final_colors={(596,601):blue,(595,596):green,(595,601):orange}
early_colors={(597,600):blue,(600,601):green,(597,601):orange}
basefinal=event(traces['baseline'],[595,596,601])
baseearly=event(traces['baseline'],[597,600,601])
final_origin=basefinal['positions'][0]
early_origin=baseearly['positions'][0]
for name,x in [('baseline',44),('intervention',724)]:
    nodes=comparison[name]['final_stop_nodes']
    assert nodes==([18,18,19] if name=='baseline' else [18,18,18])
    panel(name,(x,235),[595,596,601],final_origin,0.72,-0.235,final_colors,
          ['Gap: 0.501695 m' if name=='baseline' else 'Spread: 0.000000273 m (below plot resolution)',
           'Emitted stop nodes: 18, 18, 19' if name=='baseline' else 'Emitted stop nodes: 18, 18, 18'],0.1)
    text(x,494,'Final time: '+str(comparison[name]['final_event']['time']),19)
    text(x,524,'Later birth replay miss: '+('20.75 mm' if name=='baseline' else '14.86 mm'),19)
    text(x,565,'Earlier junction: 597 / 600 / 601',24,bold=True)
    e=panel(name,(x,605),[597,600,601],early_origin,0.014,0.0038,early_colors,
          ['Gap: 7.223 mm (unchanged)','Emitted stop nodes: 15, 16, 15'],0.001,True)
    assert e==baseearly
    text(x,864,'Birth replay miss before this stop: 0.369 mm',19)

text(44,908,'Color key: each line is a raw traced arc; circles mark its recorded stop.',20,bold=True)
for i,(color,caption) in enumerate([(blue,'Blue: 596/601 above; 597/600 below'),
                                  (green,'Green: 595/596 above; 600/601 below'),
                                  (orange,'Orange: fast child 595/601 above; 597/601 below')]):
    yy=942+i*29
    draw.line((45,yy+10,77,yy+10),fill=color,width=4)
    text(87,yy,caption,19)
text(44,1036,'Red brackets measure endpoint separation. They are not skeleton segments.',19,color='#ae3141')
text(44,1072,'The final join looks better, but the earlier gap survives and both constructions fail.',23,bold=True)
text(44,1108,'No tolerance or repair is approved by this picture. Source-support errors are not shown here.',19)
text(44,1143,'Coordinates are in metres; times are wavefront-time units. End markers are enlarged for visibility.',18,color=muted)
im.save(D/'visual-comparison.png')
print(D/'visual-comparison.png')
