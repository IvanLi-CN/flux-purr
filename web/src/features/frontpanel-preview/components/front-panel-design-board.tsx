import {
  frontPanelDefaultThresholdsC,
  frontPanelPalette,
  frontPanelTemperatureColors,
  frontPanelTypography,
} from '../design-tokens'
import { frontPanelStoryStates } from '../mock-data'
import { FrontPanelDisplay } from './front-panel-display'

const paletteEntries = [
  ['Background', frontPanelPalette.bg],
  ['Panel', frontPanelPalette.panel],
  ['Panel Strong', frontPanelPalette.panelStrong],
  ['Divider', frontPanelPalette.border],
  ['Primary Text', frontPanelPalette.text],
  ['Muted Text', frontPanelPalette.muted],
  ['Disabled', frontPanelPalette.disabled],
  ['Accent', frontPanelPalette.accent],
  ['Success', frontPanelPalette.success],
  ['Warning', frontPanelPalette.warning],
  ['Info Cyan', frontPanelPalette.cyan],
] as const

const fontFamily = '"Space Grotesk", system-ui, sans-serif'

const cardStyle = {
  background: 'rgba(15, 23, 42, 0.92)',
  border: '1px solid #1e293b',
  borderRadius: '28px',
  padding: '24px',
} as const

export function FrontPanelDesignBoard() {
  return (
    <section
      style={{
        width: '1320px',
        margin: '0 auto',
        borderRadius: '36px',
        border: '1px solid #1e293b',
        background: 'rgba(2, 6, 23, 0.95)',
        padding: '32px',
        color: '#f8fafc',
        boxShadow: '0 30px 80px rgba(2, 6, 23, 0.45)',
        fontFamily,
      }}
    >
      <header
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'flex-end',
          gap: '32px',
          borderBottom: '1px solid #1e293b',
          paddingBottom: '24px',
        }}
      >
        <div style={{ maxWidth: '760px' }}>
          <p
            style={{
              margin: 0,
              fontSize: '12px',
              fontWeight: 600,
              letterSpacing: '0.32em',
              textTransform: 'uppercase',
              color: '#67e8f9',
            }}
          >
            Flux Purr Front Panel
          </p>
          <h1 style={{ margin: '14px 0 12px', fontSize: '42px', lineHeight: 1.05 }}>
            160×50 UI design spec
          </h1>
          <p style={{ margin: 0, fontSize: '15px', lineHeight: 1.7, color: '#cbd5e1' }}>
            Use the Dashboard as the canonical reference interface: large set temperature on the
            left, heater and fan mock status on the right, and a compact status bar at the bottom.
            Key Test, menu, and child pages inherit the same palette, bitmap typography, and spacing
            rules.
          </p>
        </div>

        <div
          style={{
            display: 'grid',
            gridTemplateColumns: 'repeat(2, minmax(0, 1fr))',
            gap: '12px',
            minWidth: '300px',
          }}
        >
          {[
            ['Logical Size', '160 × 50 px'],
            ['Density Rule', '≤ 4 text rows'],
          ].map(([label, value]) => (
            <div
              key={label}
              style={{
                background: 'rgba(15, 23, 42, 0.9)',
                border: '1px solid #1e293b',
                borderRadius: '18px',
                padding: '16px',
              }}
            >
              <p
                style={{
                  margin: 0,
                  fontSize: '11px',
                  textTransform: 'uppercase',
                  letterSpacing: '0.18em',
                  color: '#94a3b8',
                }}
              >
                {label}
              </p>
              <p
                style={{
                  margin: '8px 0 0',
                  fontSize: '24px',
                  fontWeight: 700,
                  color: '#f8fafc',
                }}
              >
                {value}
              </p>
            </div>
          ))}
        </div>
      </header>

      <div
        style={{
          display: 'grid',
          gridTemplateColumns: '1.08fr 0.92fr',
          gap: '24px',
          marginTop: '32px',
        }}
      >
        <div style={{ display: 'grid', gap: '24px' }}>
          <section style={cardStyle}>
            <div
              style={{
                display: 'flex',
                justifyContent: 'space-between',
                alignItems: 'flex-end',
                gap: '24px',
                marginBottom: '20px',
              }}
            >
              <div>
                <p
                  style={{
                    margin: 0,
                    fontSize: '11px',
                    textTransform: 'uppercase',
                    letterSpacing: '0.22em',
                    color: '#94a3b8',
                  }}
                >
                  Reference interface
                </p>
                <h2 style={{ margin: '8px 0 0', fontSize: '28px', lineHeight: 1.1 }}>Dashboard</h2>
              </div>
              <p style={{ margin: 0, fontSize: '14px', color: '#94a3b8' }}>
                Core visual language lives here
              </p>
            </div>

            <div style={{ display: 'grid', gap: '18px' }}>
              <div>
                <p
                  style={{
                    margin: '0 0 10px',
                    fontSize: '11px',
                    textTransform: 'uppercase',
                    letterSpacing: '0.18em',
                    color: '#94a3b8',
                  }}
                >
                  Reference render
                </p>
                <FrontPanelDisplay
                  screen={frontPanelStoryStates.dashboard}
                  scale={5}
                  showFrame={false}
                  showMeta={false}
                />
              </div>

              <div
                style={{
                  display: 'grid',
                  gridTemplateColumns: 'repeat(3, minmax(0, 1fr))',
                  gap: '12px',
                }}
              >
                {[
                  ['Set temp', 'Left-dominant 7-segment setpoint value'],
                  ['Mock toggles', 'Heater and fan state stay on the right stack'],
                  ['Bottom cue', 'Thin bar reinforces dashboard runtime state'],
                ].map(([label, note]) => (
                  <div
                    key={label}
                    style={{
                      borderRadius: '18px',
                      border: '1px solid #1e293b',
                      background: 'rgba(2, 6, 23, 0.7)',
                      padding: '14px 16px',
                    }}
                  >
                    <p style={{ margin: 0, fontSize: '14px', fontWeight: 700 }}>{label}</p>
                    <p
                      style={{
                        margin: '8px 0 0',
                        fontSize: '13px',
                        color: '#94a3b8',
                        lineHeight: 1.5,
                      }}
                    >
                      {note}
                    </p>
                  </div>
                ))}
              </div>
            </div>
          </section>

          <section style={cardStyle}>
            <p
              style={{
                margin: 0,
                fontSize: '11px',
                textTransform: 'uppercase',
                letterSpacing: '0.22em',
                color: '#94a3b8',
              }}
            >
              Layout rules
            </p>
            <div style={{ display: 'grid', gap: '12px', marginTop: '16px' }}>
              {[
                'Dashboard is the baseline: left = set temperature, right = compact heater/fan stack, bottom = thin runtime cue bar.',
                'Default safe area is 4 px inside any panel group; avoid adding decorative outer borders on the screen edge.',
                'Key Test keeps the five-way diagram white at rest; short = Success, double = Accent, long = Info Cyan.',
                'Preset page uses M1~M10: selected slot = accent, enabled slot = primary text, disabled slot = muted gray.',
                'All mock pages keep short center exit and long center fallback exit unless the dashboard binds center long to the menu.',
              ].map((rule) => (
                <div
                  key={rule}
                  style={{
                    borderRadius: '18px',
                    border: '1px solid #1e293b',
                    background: 'rgba(2, 6, 23, 0.7)',
                    padding: '14px 16px',
                    fontSize: '14px',
                    lineHeight: 1.6,
                    color: '#cbd5e1',
                  }}
                >
                  {rule}
                </div>
              ))}
            </div>
          </section>
        </div>

        <div style={{ display: 'grid', gap: '24px' }}>
          <section style={cardStyle}>
            <p
              style={{
                margin: 0,
                fontSize: '11px',
                textTransform: 'uppercase',
                letterSpacing: '0.22em',
                color: '#94a3b8',
              }}
            >
              Palette
            </p>
            <div
              style={{
                display: 'grid',
                gridTemplateColumns: 'repeat(2, minmax(0, 1fr))',
                gap: '12px',
                marginTop: '16px',
              }}
            >
              {paletteEntries.map(([label, color]) => (
                <div
                  key={label}
                  style={{
                    borderRadius: '18px',
                    border: '1px solid #1e293b',
                    background: 'rgba(2, 6, 23, 0.7)',
                    padding: '12px',
                  }}
                >
                  <div
                    style={{
                      height: '48px',
                      borderRadius: '12px',
                      border: '1px solid rgba(255,255,255,0.1)',
                      backgroundColor: color,
                    }}
                  />
                  <p style={{ margin: '12px 0 2px', fontSize: '14px', fontWeight: 700 }}>{label}</p>
                  <p
                    style={{
                      margin: 0,
                      fontSize: '12px',
                      color: '#94a3b8',
                      fontFamily: 'ui-monospace, SFMono-Regular, monospace',
                      textTransform: 'uppercase',
                    }}
                  >
                    {color}
                  </p>
                </div>
              ))}
            </div>
          </section>

          <section style={cardStyle}>
            <p
              style={{
                margin: 0,
                fontSize: '11px',
                textTransform: 'uppercase',
                letterSpacing: '0.22em',
                color: '#94a3b8',
              }}
            >
              Typography
            </p>
            <div style={{ display: 'grid', gap: '12px', marginTop: '16px' }}>
              {frontPanelTypography.map((item) => (
                <div
                  key={item.name}
                  style={{
                    borderRadius: '18px',
                    border: '1px solid #1e293b',
                    background: 'rgba(2, 6, 23, 0.7)',
                    padding: '14px 16px',
                  }}
                >
                  <div
                    style={{
                      display: 'flex',
                      justifyContent: 'space-between',
                      alignItems: 'flex-start',
                      gap: '16px',
                    }}
                  >
                    <div>
                      <p style={{ margin: 0, fontSize: '15px', fontWeight: 700 }}>{item.name}</p>
                      <p style={{ margin: '6px 0 0', fontSize: '14px', color: '#cbd5e1' }}>
                        {item.usage}
                      </p>
                    </div>
                    <p
                      style={{
                        margin: 0,
                        fontSize: '12px',
                        color: '#67e8f9',
                        fontFamily: 'ui-monospace, SFMono-Regular, monospace',
                        textTransform: 'uppercase',
                      }}
                    >
                      {item.spec}
                    </p>
                  </div>
                </div>
              ))}
            </div>
          </section>

          <section style={cardStyle}>
            <p
              style={{
                margin: 0,
                fontSize: '11px',
                textTransform: 'uppercase',
                letterSpacing: '0.22em',
                color: '#94a3b8',
              }}
            >
              Temperature color states
            </p>
            <div style={{ display: 'grid', gap: '12px', marginTop: '16px' }}>
              <div
                style={{
                  display: 'grid',
                  gridTemplateColumns: 'repeat(5, minmax(0, 1fr))',
                  gap: '12px',
                }}
              >
                {frontPanelTemperatureColors.map((color, index) => (
                  <div
                    key={color}
                    style={{
                      borderRadius: '18px',
                      border: '1px solid #1e293b',
                      background: 'rgba(2, 6, 23, 0.7)',
                      padding: '12px',
                      textAlign: 'center',
                    }}
                  >
                    <div
                      style={{
                        height: '40px',
                        borderRadius: '12px',
                        border: '1px solid rgba(255,255,255,0.1)',
                        backgroundColor: color,
                      }}
                    />
                    <p
                      style={{
                        margin: '10px 0 0',
                        fontSize: '12px',
                        fontWeight: 700,
                        letterSpacing: '0.18em',
                        textTransform: 'uppercase',
                      }}
                    >
                      T{index + 1}
                    </p>
                  </div>
                ))}
              </div>
              <div
                style={{
                  borderRadius: '18px',
                  border: '1px solid #1e293b',
                  background: 'rgba(2, 6, 23, 0.7)',
                  padding: '14px 16px',
                  fontSize: '14px',
                  lineHeight: 1.6,
                  color: '#cbd5e1',
                }}
              >
                Threshold variables (editable later):{' '}
                <span
                  style={{
                    color: '#f8fafc',
                    fontFamily: 'ui-monospace, SFMono-Regular, monospace',
                  }}
                >
                  [{frontPanelDefaultThresholdsC.join(', ')}] ℃
                </span>
              </div>
            </div>
          </section>
        </div>
      </div>
    </section>
  )
}
