import { createFileRoute } from "@tanstack/react-router";
import RelayApp from "@/features/relay/app";

export const Route = createFileRoute("/")({ component: RelayApp });
