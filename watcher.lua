function is_rust(p)
   if p:find("target") ~= nil then return false end
   return p:ext() == "rs" or p:ext() == "toml"
end

return {
   {
      should_run = is_rust,
      redirect_stderr = "/tmp/cargo.err",
      commands = {
         -- {
            -- name = "Running cargo check",
            -- command = "cargo check --color=always",
         -- },
         -- {
            -- name = "Running cargo test",
            -- command = "cargo test --color=always",
         -- },
         -- {
            -- name = "Running cargo build",
            -- command = "cargo build --color=always",
         -- },
         -- {
            -- name = "Running cargo clippy",
            -- command = "cargo clippy --color=always -- -W clippy::pedantic",
         -- },
         {
            name = "Running cargo bench",
            command = "cargo bench --color=always",
         },
      }
   },
}
