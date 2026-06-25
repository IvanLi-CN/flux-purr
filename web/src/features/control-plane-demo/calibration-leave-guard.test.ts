import { describe, expect, it } from 'vitest'
import {
  asCalibrationWorkbenchMode,
  shouldBlockCalibrationDeviceChange,
  shouldBlockCalibrationViewChange,
  shouldBlockCalibrationWorkspaceTabChange,
} from './calibration-leave-guard'

describe('calibration leave guard', () => {
  it('maps live calibration modes back to workbench tabs', () => {
    expect(asCalibrationWorkbenchMode('off')).toBeNull()
    expect(asCalibrationWorkbenchMode('heater_curve')).toBe('heater_curve')
    expect(asCalibrationWorkbenchMode('rtd_adc')).toBe('rtd_adc')
    expect(asCalibrationWorkbenchMode('vin_adc')).toBe('vin_adc')
  })

  it('blocks leaving the calibration view while a calibration mode is armed', () => {
    expect(shouldBlockCalibrationViewChange('rtd_adc', 'calibration', 'dashboard')).toBe(true)
    expect(shouldBlockCalibrationViewChange('off', 'calibration', 'dashboard')).toBe(false)
    expect(shouldBlockCalibrationViewChange('rtd_adc', 'dashboard', 'settings')).toBe(false)
    expect(shouldBlockCalibrationViewChange('rtd_adc', 'calibration', 'calibration')).toBe(false)
  })

  it('blocks device changes only from the calibration view while a mode is armed', () => {
    expect(shouldBlockCalibrationDeviceChange('heater_curve', 'calibration')).toBe(true)
    expect(shouldBlockCalibrationDeviceChange('heater_curve', 'dashboard')).toBe(false)
    expect(shouldBlockCalibrationDeviceChange('off', 'calibration')).toBe(false)
  })

  it('blocks switching away from the currently armed calibration workspace tab', () => {
    expect(shouldBlockCalibrationWorkspaceTabChange('rtd_adc', 'rtd_adc', 'vin_adc')).toBe(true)
    expect(shouldBlockCalibrationWorkspaceTabChange('rtd_adc', 'vin_adc', 'heater_curve')).toBe(
      false
    )
    expect(shouldBlockCalibrationWorkspaceTabChange('off', 'rtd_adc', 'vin_adc')).toBe(false)
    expect(
      shouldBlockCalibrationWorkspaceTabChange('heater_curve', 'heater_curve', 'heater_curve')
    ).toBe(false)
  })
})
