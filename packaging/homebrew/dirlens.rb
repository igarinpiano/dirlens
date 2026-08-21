class Dirlens < Formula
  desc "Project map for filesystem structure and code intelligence"
  homepage "https://github.com/igarinpiano/dirlens"
  url "https://github.com/igarinpiano/dirlens/archive/refs/tags/v1.2.22.tar.gz"
  sha256 "acf9ac667106ce8d4758ef861c88ad4c1c53bd8277cb81dc53ec0fa9faefbd47"
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
