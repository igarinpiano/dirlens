class Dirlens < Formula
  desc "Project map for filesystem structure and code intelligence"
  homepage "https://github.com/igarinpiano/dirlens"
  url "https://github.com/igarinpiano/dirlens/archive/refs/tags/v1.2.21.tar.gz"
  sha256 "7582dcf6f5180039c81568737af6314f7e354c0931a77e55392bbc317526b3da"
  license "Apache-2.0"
  head "https://github.com/igarinpiano/dirlens.git", branch: "main"

  depends_on "rust" => :build

  def install
    cd "rust" do
      system "cargo", "install", *std_cargo_args(path: "crates/dirlens-cli")
    end

    generate_completions_from_executable bin/"dirlens", "--completions"
    (man1/"dirlens.1").write Utils.safe_popen_read(bin/"dirlens", "--man")
  end

  test do
    (testpath/"sample.txt").write("hello\n")
    output = shell_output("#{bin}/dirlens --agent #{testpath}")
    assert_match "sample.txt", output
  end
end
