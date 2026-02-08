import type { IconType } from "react-icons";
import {
  PiHouseDuotone,
  PiFileTextDuotone,
  PiPlusCircleDuotone,
  PiListChecksDuotone,
  PiArrowsClockwiseDuotone,
  PiPowerDuotone,
  PiCaretDownBold,
  PiCircleFill,
  PiClockDuotone,
  PiCheckCircleFill,
  PiXCircleFill,
  PiMagnifyingGlassDuotone,
  PiXBold,
} from "react-icons/pi";

export const iconLibrary: Record<string, IconType> = {
  home: PiHouseDuotone,
  fileText: PiFileTextDuotone,
  plusCircle: PiPlusCircleDuotone,
  listChecks: PiListChecksDuotone,
  refresh: PiArrowsClockwiseDuotone,
  power: PiPowerDuotone,
  chevronDown: PiCaretDownBold,
  circle: PiCircleFill,
  clock: PiClockDuotone,
  checkCircle: PiCheckCircleFill,
  xCircle: PiXCircleFill,
  search: PiMagnifyingGlassDuotone,
  close: PiXBold,
};
