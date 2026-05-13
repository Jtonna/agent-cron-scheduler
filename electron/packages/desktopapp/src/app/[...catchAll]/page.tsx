import NotFound from "../not-found";

export async function generateStaticParams() {
  return [{ catchAll: ["_"] }];
}

export default function CatchAllPage() {
  return <NotFound />;
}
