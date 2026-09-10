// The OS Builder's child routes, in one table (four-tab design § 2).
//
// `App.tsx` renders this under `/os-builder`, and the router tests render the
// same function — so a redirect that exists in the application exists in the
// test, and one that is deleted fails a test rather than silently sending a
// remembered URL to the home screen through the `*` catch-all.
//
// **Retired segments redirect, never 404.** `kaynak`, `paketler`,
// `amiga-kurulum` and `ilk-acilis` were the five-step lane's routes until
// 2026-09-09; a link in the operation log, a remembered position and a
// person's habit still name them. A redirect is the honest form of "this
// moved".
//
// **This is a function that returns elements, not a component**, and it is
// called — `{osBuilderRoutes()}` — rather than mounted as `<OsBuilderRoutes/>`.
// Measured, not assumed: `<Routes>` builds its table with
// `createRoutesFromChildren`, which walks the children it is *given*; a
// component element is never rendered first, so a component returning a
// fragment of `<Route>`s fails with "[OsBuilderRoutes] is not a <Route>
// component" (react-router 7.18.2, this tree, 2026-09-09). An array of
// `<Route>` elements is flattened by `React.Children` and walked, which is
// why the table below is mapped rather than nested.

import type { ReactElement } from "react";
import { Navigate, Route } from "react-router-dom";

import {
  StepBirimler,
  StepDerle,
  StepDosyalar,
  StepKart,
  StepMakine,
  StepSecim,
} from "@/pages/osbuilder/steps";
import { StepHedef } from "@/pages/OsBuilder";

/** Every path under `/os-builder`, live steps first and redirects after. */
const CHILDREN: { path: string; element: ReactElement }[] = [
  { path: "hedef", element: <StepHedef /> },
  { path: "dosyalar", element: <StepDosyalar /> },
  { path: "secim", element: <StepSecim /> },
  { path: "makine", element: <StepMakine /> },
  { path: "derle", element: <StepDerle /> },
  { path: "kart", element: <StepKart /> },
  { path: "birimler", element: <StepBirimler /> },
  // The retired five-step lane. `replace`, so back does not bounce.
  { path: "kaynak", element: <Navigate to="/os-builder/dosyalar" replace /> },
  { path: "paketler", element: <Navigate to="/os-builder/secim" replace /> },
  { path: "amiga-kurulum", element: <Navigate to="/os-builder/secim" replace /> },
  { path: "ilk-acilis", element: <Navigate to="/os-builder/secim" replace /> },
];

/** The children of `<Route path="os-builder">`. Call it; do not mount it. */
export function osBuilderRoutes(): ReactElement[] {
  return [
    <Route key="index" index element={<Navigate to="hedef" replace />} />,
    ...CHILDREN.map(({ path, element }) => <Route key={path} path={path} element={element} />),
  ];
}
