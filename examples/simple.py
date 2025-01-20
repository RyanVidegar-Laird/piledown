from pathlib import Path
from piledown import PileParams, Strand, LibFragmentType
import pyarrow as pa
import seaborn as sns
import matplotlib.pyplot as plt

input_bam = Path() / "tests" / "data" / "SRR21778056-sorted-subsample.bam"
piledown = PileParams(
    input_bam,
    "chr1:14000-25000",
    Strand.Reverse,
    LibFragmentType.Isr,
)

res = piledown.generate()
res = res.set_column(4, "down", pa.compute.multiply(res["down"].cast("int64"), -1))

sns.lineplot(res, x="pos", y="up")
sns.lineplot(res, x="pos", y="down")
plt.show()
