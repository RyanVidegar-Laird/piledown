rule star_align:
    input:
        r1="results/fastq/{sample}_1.fastq.gz",
        r2="results/fastq/{sample}_2.fastq.gz",
        index="results/reference/star_index",
        gtf="results/reference/gencode.v{release}.annotation.gtf.gz".format(
            release=config["gencode_release"]
        ),
    output:
        bam="results/aligned/{sample}.sorted.bam",
    params:
        tmpdir="results/aligned/{sample}_star_tmp/",
    threads: config["star"]["threads"]
    shell:
        """
        STAR --runMode alignReads \
            --runThreadN {threads} \
            --readFilesIn {input.r1} {input.r2} \
            --readFilesCommand zcat \
            --genomeDir {input.index} \
            --sjdbGTFfile {input.gtf} \
            --sjdbOverhang {config[star][sjdb_overhang]} \
            --outSAMtype BAM SortedByCoordinate \
            --outSAMmode NoQS \
            --outSAMunmapped Within \
            --outFileNamePrefix {params.tmpdir}
        mv {params.tmpdir}Aligned.sortedByCoord.out.bam {output.bam}
        rm -rf {params.tmpdir}
        """


rule samtools_index:
    input:
        "results/aligned/{sample}.sorted.bam",
    output:
        "results/aligned/{sample}.sorted.bam.bai",
    threads: 4
    shell:
        "samtools index -@ {threads} {input}"
