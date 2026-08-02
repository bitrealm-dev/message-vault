import type { NextRequest } from "next/server";
import { NextResponse } from "next/server";

import { ACCOUNT_COOKIE } from "@/lib/accountCookie";

const PUBLIC_PREFIXES = ["/login", "/api/auth"];

function isPublicPath(pathname: string): boolean {
  return PUBLIC_PREFIXES.some(
    (prefix) => pathname === prefix || pathname.startsWith(`${prefix}/`),
  );
}

export function proxy(request: NextRequest) {
  const { pathname } = request.nextUrl;
  const accountId = request.cookies.get(ACCOUNT_COOKIE)?.value?.trim();

  if (isPublicPath(pathname)) {
    // Already signed in — don't leave people stuck on the login screen
    // after a stale client navigation. Incomplete profiles land on /
    // and server pages redirect to /onboarding.
    if (pathname === "/login" && accountId) {
      const home = request.nextUrl.clone();
      home.pathname = "/";
      return NextResponse.redirect(home);
    }
    return NextResponse.next();
  }

  if (!accountId) {
    const login = request.nextUrl.clone();
    login.pathname = "/login";
    return NextResponse.redirect(login);
  }

  // /onboarding requires a vault cookie (authenticated); completeness
  // is enforced in the page / withServerAccount redirects.
  return NextResponse.next();
}

export const config = {
  matcher: [
    "/((?!_next/static|_next/image|favicon.ico|.*\\.(?:svg|png|jpg|jpeg|gif|webp)$).*)",
  ],
};
