import { describe, expect, it } from 'vitest'
import {
  resolveCalibrationSliderValue,
  validateCalibrationSliderText,
} from './calibration-slider-value'

describe('calibration slider value resolution', () => {
  it('uses the slider minimum when the text field is empty', () => {
    expect(resolveCalibrationSliderValue('', 0, 2800)).toBe(0)
  })

  it('rounds numeric text and clamps it to the slider range', () => {
    expect(resolveCalibrationSliderValue('917.6', 0, 2800)).toBe(918)
    expect(resolveCalibrationSliderValue('-1', 0, 2800)).toBe(0)
    expect(resolveCalibrationSliderValue('3000', 0, 2800)).toBe(2800)
  })

  it('validates raw text before slider clamping', () => {
    expect(validateCalibrationSliderText('', 0, 2800)).toBeNull()
    expect(validateCalibrationSliderText('917.6', 0, 2800)).toBeNull()
    expect(validateCalibrationSliderText('-1', 0, 2800)).toBe('请输入 0-2800 范围内的数值。')
    expect(validateCalibrationSliderText('3000', 0, 2800)).toBe('请输入 0-2800 范围内的数值。')
  })
})
