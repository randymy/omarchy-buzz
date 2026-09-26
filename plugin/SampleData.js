// Synthetic, public fixtures. Nothing here represents a connected relay.
function snapshot() {
  return {
    version: 1,
    instanceId: "sample",
    generation: 0,
    type: "snapshot",
    payload: {
      source: "sample",
      connection: "preview",
      community: "Sample community",
      rooms: [
        { id: "sample-general", name: "general", description: "People and agents, in one conversation" },
        { id: "sample-development", name: "development", description: "Shared project history" }
      ],
      messages: [
        { id: "sample-1", roomId: "sample-general", author: "Alex", role: "Person", time: "09:41", text: "Welcome to our shared workspace. This is a sample conversation." },
        { id: "sample-2", roomId: "sample-general", author: "Research agent", role: "Agent · sample", time: "09:42", text: "People and agents will share the same room history. No agent is running in this preview." },
        { id: "sample-3", roomId: "sample-development", author: "Alex", role: "Person", time: "10:03", text: "The first milestone is a native bar widget and a keyboard-friendly panel." },
        { id: "sample-4", roomId: "sample-development", author: "Code agent", role: "Agent · sample", time: "10:04", text: "Relay connectivity, signed mentions, and real activity will follow in later milestones." }
      ]
    }
  }
}
