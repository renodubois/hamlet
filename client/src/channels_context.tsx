import { createContext, useContext, type JSX, type Resource } from "solid-js";
import { type Channel } from "./api";

const ChannelsContext = createContext<Resource<Channel[]>>();

export function ChannelsProvider(props: { channels: Resource<Channel[]>; children: JSX.Element }) {
  return (
    <ChannelsContext.Provider value={props.channels}>
      {props.children}
    </ChannelsContext.Provider>
  );
}

export function useChannels() {
  const ctx = useContext(ChannelsContext);
  if (!ctx) throw new Error("useChannels must be used inside ChannelsProvider");
  return ctx;
}
