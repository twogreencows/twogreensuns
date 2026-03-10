"use client";

import { Navbar, NavbarBrand, NavbarContent, NavbarItem, Link } from "@heroui/react";
import { usePathname } from "next/navigation";

export function AppNavBar() {
  const path = usePathname();

  return (
    <Navbar isBordered>
      <NavbarBrand>
        <p className="font-bold text-inherit">🌿 TwoGreenSuns</p>
      </NavbarBrand>
      <NavbarContent className="gap-4" justify="center">
        <NavbarItem isActive={path === "/dashboard"}>
          <Link href="/dashboard" color={path === "/dashboard" ? "primary" : "foreground"}>
            Dashboard
          </Link>
        </NavbarItem>
        <NavbarItem isActive={path === "/tesla"}>
          <Link href="/tesla" color={path === "/tesla" ? "primary" : "foreground"}>
            Tesla
          </Link>
        </NavbarItem>
        <NavbarItem isActive={path === "/settings"}>
          <Link href="/settings" color={path === "/settings" ? "primary" : "foreground"}>
            Settings
          </Link>
        </NavbarItem>
      </NavbarContent>
    </Navbar>
  );
}
