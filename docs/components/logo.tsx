import Link from 'next/link';

// Brand logos from the Mintlify config: light variant for light mode,
// dark variant for dark mode.
export function Logo() {
  return (
    <Link href="/" className="inline-flex items-center gap-2 font-semibold">
      <img
        src="/images/kagi-cli-logo-light.svg"
        alt="kagi CLI"
        className="h-7 w-auto max-w-[180px] object-contain object-left dark:hidden"
      />
      <img
        src="/images/kagi-cli-logo-dark.svg"
        alt="kagi CLI"
        className="hidden h-7 w-auto max-w-[180px] object-contain object-left dark:block"
      />
    </Link>
  );
}
