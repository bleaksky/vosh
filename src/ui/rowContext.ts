import { createContext, useContext } from 'react';

// A Row hands its ids to the control inside it, so the row label
// labels the control and the description describes it without either
// side passing ids around. A control given its own id or label keeps
// it, which is how a row with two controls avoids a duplicate id.

export interface RowIds {
  /** The id the row's <label> points at. The first control takes it. */
  controlId: string;
  /** The id of the row label, for controls a <label> cannot name
   *  (a Segmented group). */
  labelId: string;
  /** The id of the row description, when the row has one. */
  descriptionId: string | undefined;
}

export const RowContext = createContext<RowIds | null>(null);

export function useRowIds(): RowIds | null {
  return useContext(RowContext);
}
